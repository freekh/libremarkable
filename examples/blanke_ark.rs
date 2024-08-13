mod blanke_ark_lib;

use std::sync::mpsc::channel;

use blanke_ark_lib::message::{ChunkCoordinates, GlobalCoordinates, Path, Subscription};
use futures::stream::StreamExt;
use futures::SinkExt;
use libremarkable::framebuffer::common::{
    display_temp, dither_mode, waveform_mode, DRAWING_QUANT_BIT,
};
use libremarkable::framebuffer::core::Framebuffer;
use libremarkable::framebuffer::{FramebufferDraw, FramebufferRefresh, PartialRefreshMode};
use libremarkable::input::ev::EvDevContext;
use libremarkable::input::WacomEvent;
use libremarkable::{appctx, input};
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::protocol::Message;

#[tokio::main]
async fn main() {
    env_logger::init();
    let mut app: appctx::ApplicationContext<'_> = appctx::ApplicationContext::default();

    app.clear(true);

    let (ws_stream, _) = connect_async("wss://ark.blank.no/ws")
        .await
        .expect("Failed to connect");
    let (mut write, mut read) = ws_stream.split();
    println!("Connected to the server");
    println!("{:?}", app.get_dimensions());
    let chunk_size = 1404f32;
    let framebuffer = app.get_framebuffer_ref();

    tokio::spawn(async move {
        println!("Listening for messages");
        while let Some(msg) = read.next().await {
            println!("{:?}", msg);
            if let Ok(Message::Binary(data)) = msg {
                let message: blanke_ark_lib::message::Message =
                    postcard::from_bytes(&data).unwrap();
                match message {
                    blanke_ark_lib::message::Message::Draw(draw_message) => {
                        match draw_message {
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
                        }
                    }
                    blanke_ark_lib::message::Message::Subscribe(subscription) => {
                        println!("Received subscription: {:?}!!?!?!", subscription);
                    }
                }
            }
        }
        println!("Out for messages");
    });

    write
        .send(Message::Binary(
            postcard::to_allocvec(&blanke_ark_lib::message::Message::Subscribe(
                Subscription::from(ChunkCoordinates { x: 0, y: 0 }),
            ))
            .unwrap(),
        ))
        .await
        .unwrap();

    tokio::spawn(async move {
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
                            write.send(msg).await.unwrap();
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
