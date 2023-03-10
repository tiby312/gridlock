use super::*;

///Common data sent from the main thread to the worker.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum MEvent {
    CanvasMouseMove {
        x: f32,
        y: f32,
    },
    CanvasMouseDown {
        x: f32,
        y: f32,
    },
    TouchMove {
        touches: scroll::Touches,
    },
    TouchDown {
        touches: scroll::Touches,
    },
    TouchEnd {
        touches: scroll::Touches,
    },
    EndTurn,
    CanvasMouseUp,
    CanvasMouseLeave,
    ButtonClick,
    ShutdownClick,
    Resize {
        canvasx: u32,
        canvasy: u32,
        x: f32,
        y: f32,
    },
}
impl MEvent {
    fn some(self) -> Option<Self> {
        Some(self)
    }
}

//convert DOM coordinate to canvas relative coordinate
fn convert_coord(canvas: &web_sys::EventTarget, event: &web_sys::Event) -> [f32; 2] {
    shogo::simple2d::convert_coord(
        canvas.dyn_ref().unwrap_throw(),
        event.dyn_ref().unwrap_throw(),
    )
}

fn convert_coord_touch(canvas: &web_sys::EventTarget, event: &web_sys::Event) -> scroll::Touches {
    event.prevent_default();
    event.stop_propagation();
    convert_coord_touch_inner(canvas, event.dyn_ref().unwrap_throw())
}

///
/// Convert a mouse event to a coordinate for simple2d.
///
pub fn convert_coord_touch_inner(
    canvas: &web_sys::EventTarget,
    e: &web_sys::TouchEvent,
) -> scroll::Touches {
    let canvas: &web_sys::HtmlElement = canvas.dyn_ref().unwrap_throw();
    let rect = canvas.get_bounding_client_rect();

    let canvas_width: f64 = canvas
        .get_attribute("width")
        .unwrap_throw()
        .parse()
        .unwrap_throw();
    let canvas_height: f64 = canvas
        .get_attribute("height")
        .unwrap_throw()
        .parse()
        .unwrap_throw();

    let scalex = canvas_width / rect.width();
    let scaley = canvas_height / rect.height();

    let touches = e.touches();

    let mut k = scroll::Touches {
        all: [(0, 0.0, 0.0); 4],
        count: 0,
    };

    for a in (0..touches.length()).take(4) {
        let touch = touches.get(a).unwrap();
        let x = touch.client_x() as f64;
        let y = touch.client_y() as f64;
        //let rx = touch.radius_x() as f64;
        //let ry = touch.radius_y() as f64;
        let [x, y] = [
            (x * scalex - rect.left() * scalex),
            (y * scaley - rect.top() * scaley),
        ];

        let id = touch.identifier();
        k.all[k.count] = (id, x as f32, y as f32);
        k.count += 1;
        //ans.push([x as f32, y as f32]);
    }
    k
}

pub fn text_texture(text: &str, width: usize, height: usize) -> web_sys::HtmlCanvasElement {
    let canvas = gloo::utils::document().create_element("canvas").unwrap();
    let canvas: web_sys::HtmlCanvasElement = canvas.dyn_into().unwrap();
    let ctx = canvas.get_context("2d").unwrap().unwrap();
    let ctx: web_sys::CanvasRenderingContext2d = ctx.dyn_into().unwrap();

    canvas.set_width(width as u32);
    canvas.set_height(height as u32);
    ctx.set_font(text);
    ctx.set_text_align("center");
    ctx.set_text_baseline("middle");
    ctx.set_fill_style(&JsValue::from_str("black"));
    ctx.clear_rect(0., 0., width as f64, height as f64);
    ctx.fill_text(text, width as f64 / 2., height as f64 / 2.)
        .unwrap();
    canvas
}

#[wasm_bindgen]
pub async fn main_entry() {
    use futures::StreamExt;

    log!("demo start");

    let (canvas, button, endturn) = (
        utils::get_by_id_canvas("mycanvas"),
        utils::get_by_id_elem("mybutton"),
        utils::get_by_id_elem("endturn"),
    );

    button.set_hidden(true);

    canvas.set_width(gloo::utils::body().client_width() as u32);
    canvas.set_height(gloo::utils::body().client_height() as u32);

    let offscreen = canvas.transfer_control_to_offscreen().unwrap_throw();

    let (mut worker, mut response) =
        shogo::EngineMain::new("./gridlock_worker.js", offscreen).await;

    let _handler = worker.register_event(&canvas, "mousemove", |e| {
        let [x, y] = convert_coord(e.elem, e.event);
        MEvent::CanvasMouseMove { x, y }.some()
    });

    let _handler = worker.register_event(&canvas, "mousedown", |e| {
        let [x, y] = convert_coord(e.elem, e.event);
        MEvent::CanvasMouseDown { x, y }.some()
    });

    let _handler = worker.register_event(&canvas, "wheel", |e| {
        e.event.prevent_default();
        e.event.stop_propagation();
        None
    });

    let _handler = worker.register_event(&canvas, "mouseup", |_| MEvent::CanvasMouseUp.some());

    let _handler =
        worker.register_event(&canvas, "mouseleave", |_| MEvent::CanvasMouseLeave.some());

    let _handler = worker.register_event(&canvas, "touchstart", |e| {
        let touches = convert_coord_touch(e.elem, e.event);
        MEvent::TouchDown { touches }.some()
    });

    let _handler = worker.register_event(&canvas, "touchmove", |e| {
        let touches = convert_coord_touch(e.elem, e.event);
        MEvent::TouchMove { touches }.some()
    });

    let _handler = worker.register_event(&canvas, "touchend", |e| {
        let touches = convert_coord_touch(e.elem, e.event);
        MEvent::TouchEnd { touches }.some()
    });

    let bb = button.clone();
    let _handler = worker.register_event(&button, "click", move |_| {
        log!("clicked the button!!!!!");
        bb.set_hidden(true);
        MEvent::ButtonClick.some()
    });

    let _handler = worker.register_event(&endturn, "click", move |_| {
        log!("clicked the button!!!!!");
        MEvent::EndTurn.some()
    });

    let w = gloo::utils::window();

    let _handler = worker.register_event(&w, "resize", |_| resize().some());

    //TODO make this happen on start??
    worker.post_message(resize());

    loop {
        let hay: UiButton = response.next().await.unwrap_throw();

        match hay {
            UiButton::ShowRoadUi => {
                button.set_hidden(false);
                button.set_text_content(Some("make a road?"));
            }
            UiButton::NoUi => {
                button.set_text_content(Some(""));
            }
        }
        log!(format!("main thread received={:?}", hay));
    }
    log!("main thread is closing");
}
fn resize() -> MEvent {
    let canvas = utils::get_by_id_canvas("mycanvas");
    //canvas.set_width(gloo::utils::body().client_width() as u32);
    //canvas.set_height(gloo::utils::body().client_height() as u32);

    // let width = gloo::utils::document().body().unwrap_throw().client_width();
    // let height = gloo::utils::document()
    //     .body()
    //     .unwrap_throw()
    //     .client_height();

    let width = canvas.client_width();
    let height = canvas.client_height();

    let realpixels = gloo::utils::window().device_pixel_ratio();
    log!(format!("pixel ratio:{:?}", realpixels));
    // .body.clientWidth;
    // var height=document.body.clientHeight;

    // var realToCSSPixels = window.devicePixelRatio;
    // var gl_width  = Math.floor(width  * realToCSSPixels);
    // var gl_height = Math.floor(height * realToCSSPixels);

    let gl_width = (width as f64 * realpixels).floor();
    let gl_height = (height as f64 * realpixels).floor();

    MEvent::Resize {
        canvasx: gloo::utils::body().client_width() as u32,
        canvasy: gloo::utils::body().client_height() as u32,
        x: gl_width as f32,
        y: gl_height as f32,
    }
}
