mod blanke_ark_lib;

use std::sync::Arc;

use blanke_ark_lib::message::{
    ChunkCoordinates, GlobalCoordinates, Line, Path, PathId, PathStepAction, PathStepDraw,
    PathStepEnd, Subscription,
};
use cgmath::Point2;
use futures::stream::StreamExt;
use futures::SinkExt;
use libremarkable::framebuffer::common::{
    display_temp, dither_mode, waveform_mode, DRAWING_QUANT_BIT,
};
use libremarkable::framebuffer::core::Framebuffer;
use libremarkable::framebuffer::{FramebufferDraw, FramebufferRefresh, PartialRefreshMode};
use libremarkable::input::WacomEvent;
use libremarkable::{appctx, input};
use tokio::sync::Mutex;
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::protocol::Message;

#[tokio::main]
async fn main() {
    // main_simple().await;
    main_blanke_ark().await;
}

async fn main_simple() {
    env_logger::init();
    let mut app: appctx::ApplicationContext<'_> = appctx::ApplicationContext::default();
    app.clear(true);
    let framebuffer = app.get_framebuffer_ref();

    let mut last_point: Option<Point2<i32>> = None;
    app.start_event_loop(true, true, true, |_ctx, evt| match evt {
        input::InputEvent::WacomEvent { event } => match event {
            WacomEvent::Draw {
                position,
                pressure: _,
                tilt: _,
            } => {
                let end = Point2 {
                    x: position.x as i32,
                    y: position.y as i32,
                };
                if let Some(start) = last_point {
                    println!("Drawing line from {:?} to {:?}", start, end);
                    let region = framebuffer.draw_line(
                        start,
                        end,
                        2,
                        libremarkable::framebuffer::common::color::BLACK,
                    );
                    framebuffer.partial_refresh(
                        &region,
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
                last_point = Some(end);
            }
            _ => {
                last_point = None;
            }
        },
        _ => {}
    })
}

async fn main_blanke_ark() {
    env_logger::init();
    let mut app: appctx::ApplicationContext<'_> = appctx::ApplicationContext::default();
    app.clear(true);
    let framebuffer = app.get_framebuffer_ref();

    let (ws_stream, _) = connect_async("wss://ark.blank.no/ws")
        .await
        .expect("Failed to connect");
    let (mut write, mut read) = ws_stream.split();
    println!("Connected to the server");
    println!("{:?}", app.get_dimensions());
    let chunk_size = 1404f32;
    let mut maybe_last_step_coords: Option<GlobalCoordinates> = None;
    let mut maybe_last_step_id: Option<PathId> = None;
    tokio::spawn(async move {
        println!("Listening for messages");
        while let Some(msg) = read.next().await {
            if let Ok(Message::Binary(data)) = msg {
                let message: blanke_ark_lib::message::Message =
                    postcard::from_bytes(&data).unwrap();
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
                                blanke_ark_lib::message::DrawMessage::Line(line) => {
                                    draw_line(line.from,  line.to, line.width.as_f32(), chunk_size, framebuffer);
                                }
                                _ => {
                                    println!("Received composite draw message that is not a path: {:?}", msg);
                                    return;
                                }
                            };
                        });
                            refresh(framebuffer);
                        }
                        blanke_ark_lib::message::DrawMessage::PathStepAction(step_action) => {
                            match step_action {
                                PathStepAction::Draw(step_draw) => {
                                    if let Some(last_step_coords) = maybe_last_step_coords {
                                        if let Some(last_step_id) = maybe_last_step_id {
                                            if last_step_id == step_draw.id {
                                                draw_line(
                                                    last_step_coords,
                                                    step_draw.point.clone(),
                                                    step_draw.width.as_f32(),
                                                    chunk_size,
                                                    framebuffer,
                                                );
                                            }
                                        }
                                    }
                                    maybe_last_step_id = Some(step_draw.id);
                                    maybe_last_step_coords = Some(step_draw.point.clone());
                                }
                                _ => {
                                    println!(
                                        "Received step action that is not a draw: {:?}",
                                        step_action
                                    );
                                    return;
                                }
                            };
                        }
                        blanke_ark_lib::message::DrawMessage::Line(line) => {
                            draw_line(
                                line.from,
                                line.to,
                                line.width.as_f32(),
                                chunk_size,
                                framebuffer,
                            );
                        }
                        _ => {
                            println!("Unhandled draw message: {:?}", draw_message);
                        }
                    },
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

    let write: Arc<
        Mutex<
            futures::stream::SplitSink<
                tokio_tungstenite::WebSocketStream<
                    tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
                >,
                Message,
            >,
        >,
    > = Arc::new(Mutex::new(write));
    let mut points: Vec<GlobalCoordinates> = Vec::new();
    let mut last_framebuffer_point: Option<Point2<i32>> = None;
    app.start_event_loop(true, true, true, |ctx, evt| match evt {
        input::InputEvent::WacomEvent { event } => match event {
            WacomEvent::Draw {
                position,
                pressure: _,
                tilt: _,
            } => {
                let framebuffer = ctx.get_framebuffer_ref();
                let end = Point2 {
                    x: position.x as i32,
                    y: position.y as i32,
                };
                if let Some(start) = last_framebuffer_point {
                    println!("Drawing line from {:?} to {:?}", start, end);
                    let region = framebuffer.draw_line(
                        start,
                        end,
                        2,
                        libremarkable::framebuffer::common::color::BLACK,
                    );
                    framebuffer.partial_refresh(
                        &region,
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
                last_framebuffer_point = Some(end);

                let current_point = GlobalCoordinates {
                    x: position.x as f32 / chunk_size,
                    y: position.y as f32 / chunk_size,
                };
                points.push(current_point);
            }
            _ => {
                last_framebuffer_point = None;
                if points.len() > 0 {
                    println!("Sending path with points ({})", points.len());
                    let msg = &blanke_ark_lib::message::Message::Draw(
                        blanke_ark_lib::message::DrawMessage::Path(blanke_ark_lib::message::Path {
                            points: points.clone(),
                            color: blanke_ark_lib::message::Color::RGB { r: 0, g: 0, b: 0 },
                            width: blanke_ark_lib::message::Width::from(2.0 as f32),
                        }),
                    );
                    let binary_msg = Message::Binary(postcard::to_allocvec(&msg).unwrap());
                    let write = write.clone();
                    tokio::spawn(async move {
                        write.lock().await.send(binary_msg).await.unwrap();
                    });
                }
                points = vec![];
            }
        },
        _ => {}
    });
}

fn draw_path(path: Path, chunk_size: f32, framebuffer: &mut Framebuffer) {
    path.points.windows(2).for_each(|segment| {
        let start = Point2 {
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
            path.width.as_f32() as u32,
            libremarkable::framebuffer::common::color::BLACK,
        );
    });
}

fn draw_line(
    from: blanke_ark_lib::message::GlobalCoordinates,
    to: blanke_ark_lib::message::GlobalCoordinates,
    width: f32,
    chunk_size: f32,
    framebuffer: &mut Framebuffer,
) {
    let region = framebuffer.draw_line(
        cgmath::Point2 {
            x: (from.x * chunk_size) as i32,
            y: (from.y * chunk_size) as i32,
        },
        cgmath::Point2 {
            x: (to.x * chunk_size) as i32,
            y: (to.y * chunk_size) as i32,
        },
        width as u32,
        libremarkable::framebuffer::common::color::BLACK,
    );
    framebuffer.partial_refresh(
        &region,
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
