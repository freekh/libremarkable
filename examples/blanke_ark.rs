mod blanke_ark_lib;

use std::sync::mpsc::{self, channel};
use std::thread::spawn;

use blanke_ark_lib::message::{ChunkCoordinates, GlobalCoordinates, Path, Subscription};
use evdev::InputEvent;
use libremarkable::framebuffer::common::{
    display_temp, dither_mode, waveform_mode, DRAWING_QUANT_BIT,
};
use libremarkable::framebuffer::core::Framebuffer;
use libremarkable::framebuffer::{FramebufferDraw, FramebufferRefresh, PartialRefreshMode};
use libremarkable::input::ev::EvDevContext;
use libremarkable::input::WacomEvent;
use libremarkable::{appctx, battery, image, input};
use tungstenite::{connect, Message};

fn main() {
    env_logger::init();
    let mut app: appctx::ApplicationContext<'_> = appctx::ApplicationContext::default();

    app.clear(true);

    let (mut socket, response) = connect("wss://ark.blank.no/ws").expect("Can't connect");

    println!("Connected to the server");
    // println!("Response HTTP code: {}", response.status());
    // println!("Response contains the following headers:");
    // for (ref header, _value) in response.headers() {
    //     println!("* {}", header);
    // }
    println!("{:?}", app.get_dimensions());
    let chunk_size = 1404f32;

    socket
        .send(Message::Binary(
            postcard::to_allocvec(&blanke_ark_lib::message::Message::Subscribe(
                Subscription::from(ChunkCoordinates { x: 0, y: 0 }),
            ))
            .unwrap(),
        ))
        .unwrap();
    let framebuffer = app.get_framebuffer_ref();

    let (input_sender, input_receiver) = mpsc::sync_channel(1000);

    // input thread:
    spawn(move || {
        let (input_tx, input_rx) = channel::<input::InputEvent>();
        EvDevContext::new(input::InputDevice::Wacom, input_tx).start();
        loop {
            if let Ok(event) = input_rx.recv() {
                match event {
                    input::InputEvent::WacomEvent { event } => match event {
                        WacomEvent::Draw {
                            position,
                            pressure,
                            tilt: _,
                        } => {
                            let msg = Message::Binary(
                                postcard::to_allocvec(&blanke_ark_lib::message::Message::Draw(
                                    blanke_ark_lib::message::DrawMessage::Dot(
                                        blanke_ark_lib::message::Dot {
                                            coordinates: GlobalCoordinates {
                                                x: (position.x / chunk_size),
                                                y: (position.y / chunk_size),
                                            },
                                            diam: blanke_ark_lib::message::Width::from(
                                                pressure as f64,
                                            ),
                                            color: blanke_ark_lib::message::Color::RGB {
                                                r: 0,
                                                g: 0,
                                                b: 0,
                                            },
                                        },
                                    ),
                                ))
                                .unwrap(),
                            );
                            input_sender.send(msg).unwrap();
                        }
                        _ => {
                            // println!(
                            //     "Received input event that is not a wacom event: {:?}",
                            //     event
                            // );
                        }
                    },
                    _ => {
                        // println!("Received input event that is not a wacom event");
                    }
                }
            }
        }
    });

    loop {
        let msg = socket.read();
        println!("{:?}", msg);
        if let Ok(Message::Binary(data)) = msg {
            let message: blanke_ark_lib::message::Message = postcard::from_bytes(&data).unwrap();
            match message {
                blanke_ark_lib::message::Message::Draw(draw_message) => match draw_message {
                    blanke_ark_lib::message::DrawMessage::Path(path) => {
                        draw_path(path, chunk_size, framebuffer);
                        refresh(framebuffer);
                    }
                    blanke_ark_lib::message::DrawMessage::Composite(composite) => {
                        composite.0.iter().for_each(|msg| {
                            match msg {
                                blanke_ark_lib::message::DrawMessage::Path(path) => {
                                    draw_path(path.clone(), chunk_size, framebuffer);
                                }
                                _ => {
                                    println!("Received composite draw message that is not a path");
                                    return;
                                }
                            };
                        });
                        refresh(framebuffer);
                    }
                    _ => {
                        println!(
                            "Received draw message that is not a path: {:?}",
                            draw_message
                        );
                    }
                },
                blanke_ark_lib::message::Message::Subscribe(subscription) => {
                    println!("Received subscription: {:?}!!?!?!", subscription);
                }
            }
        }
        input_receiver.try_recv().into_iter().for_each(|msg| {
            socket.send(msg).unwrap();
        });
    }
    // socket.close(None)
}

fn draw_path(path: Path, chunk_size: f32, framebuffer: &mut Framebuffer) {
    path.points.windows(2).for_each(|segment| {
        let start = cgmath::Point2 {
            x: (segment[0].x * chunk_size) as i32,
            y: (segment[0].y * chunk_size) as i32,
        };
        let end = cgmath::Point2 {
            x: (segment[1].x * chunk_size) as i32,
            y: (segment[1].y * chunk_size) as i32,
        };
        framebuffer.draw_line(
            start,
            end,
            path.width.as_f64() as u32,
            libremarkable::framebuffer::common::color::BLACK,
        );
    });
}

fn refresh(framebuffer: &mut Framebuffer) {
    framebuffer.partial_refresh(
        &libremarkable::framebuffer::common::mxcfb_rect {
            top: 0,
            left: 0,
            width: framebuffer.var_screen_info.xres,
            height: framebuffer.var_screen_info.yres,
        },
        PartialRefreshMode::Async,
        // DU mode only supports black and white colors.
        // See the documentation of the different waveform modes
        // for more information
        waveform_mode::WAVEFORM_MODE_DU,
        display_temp::TEMP_USE_REMARKABLE_DRAW,
        dither_mode::EPDC_FLAG_EXP1,
        DRAWING_QUANT_BIT,
        false,
    );
}
