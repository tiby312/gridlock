
use std::f32::consts::PI;

use axgeom::{vec2, vec2same, Vec2};
use gloo::console::log;
use gltf::image::Source;
use serde::{Deserialize, Serialize};
use shogo::simple2d;
use shogo::{simple2d::DynamicBuffer, utils};
use wasm_bindgen::prelude::*;

use duckduckgeo::grid;

pub mod matrix;
mod scroll;

const COLORS: &[[f32; 4]] = &[
    [1.0, 0.0, 0.0, 0.5],
    [0.0, 1.0, 0.0, 0.5],
    [0.0, 0.0, 1.0, 0.5],
];

///Common data sent from the main thread to the worker.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum MEvent {
    CanvasMouseMove { x: f32, y: f32 },
    CanvasMouseDown { x: f32, y: f32 },
    CanvasMouseUp,
    ButtonClick,
    ShutdownClick,
}


#[wasm_bindgen]
pub async fn main_entry() {
    console_error_panic_hook::set_once();
    
    

    use futures::StreamExt;

    log!("demo start");

    let (canvas, button, shutdown_button) = (
        utils::get_by_id_canvas("mycanvas"),
        utils::get_by_id_elem("mybutton"),
        utils::get_by_id_elem("shutdownbutton"),
    );

    let offscreen = canvas.transfer_control_to_offscreen().unwrap_throw();

    let (mut worker, mut response) =
        shogo::EngineMain::new("./gridlock_worker.js", offscreen).await;

    let _handler = worker.register_event(&canvas, "mousemove", |e| {
        let [x, y] = convert_coord(e.elem, e.event);
        MEvent::CanvasMouseMove { x, y }
    });

    let _handler = worker.register_event(&canvas, "mousedown", |e| {
        let [x, y] = convert_coord(e.elem, e.event);
        MEvent::CanvasMouseDown { x, y }
    });

    let _handler = worker.register_event(&canvas, "mouseup", |_| MEvent::CanvasMouseUp);

    let _handler = worker.register_event(&canvas, "touchstart", |e| {
        let [x, y] = convert_coord_touch(e.elem, e.event);
        MEvent::CanvasMouseDown { x, y }
    });

    let _handler = worker.register_event(&canvas, "touchend", |_| MEvent::CanvasMouseUp);

    let _handler = worker.register_event(&canvas, "touchmove", |e| {
        let [x, y] = convert_coord_touch(e.elem, e.event);
        MEvent::CanvasMouseMove { x, y }
    });

    let _handler = worker.register_event(&button, "click", |_| MEvent::ButtonClick);

    let _handler = worker.register_event(&shutdown_button, "click", |_| MEvent::ShutdownClick);

    let _: () = response.next().await.unwrap_throw();
    log!("main thread is closing");
}

#[wasm_bindgen]
pub async fn worker_entry() {
    let (mut w, ss) = shogo::EngineWorker::new().await;
    let mut frame_timer = shogo::FrameTimer::new(30, ss);

    let canvas = w.canvas();
    let ctx = simple2d::ctx_wrap(&utils::get_context_webgl2_offscreen(&canvas));
    let mut draw_sys = ctx.shader_system();
    let mut buffer = ctx.buffer_dynamic();
    let cache = &mut vec![];
    let mut walls = ctx.buffer_dynamic();

    //TODO move?
    ctx.viewport(0, 0, canvas.width() as i32, canvas.height() as i32);
    ctx.setup_alpha();

    // setup game data
    let mut color_iter = COLORS.iter().cycle().peekable();
    let game_dim = [1000.0f32, 1000.0];
    let viewport = [canvas.width() as f32, canvas.height() as f32];

    let grid_width = 32;

    let grid_viewport = duckduckgeo::grid::GridViewPort {
        origin: vec2(0.0, 0.0),
        spacing: game_dim[0] / (grid_width as f32),
    };

    let mut k = simple2d::shapes(cache);
    for x in 0..grid_width {
        let offset = if x % 2 == 0 {
            0..grid_width
        } else {
            1..grid_width - 1
        };
        for y in offset.step_by(2) {
            k.rect(simple2d::Rect {
                x: x as f32 * grid_viewport.spacing,
                y: y as f32 * grid_viewport.spacing,
                w: grid_viewport.spacing,
                h: grid_viewport.spacing,
            },-0.1);
        }
    }

    let checker = ctx.buffer_static_clear(cache);

    let mut grid_walls = grid::Grid2D::new(vec2same(grid_width));

    grid_walls.set(vec2(0, 0), true);
    grid_walls.set(vec2(2, 1), true);

    update_walls(&grid_viewport, cache, &mut walls, &grid_walls);

    let mut scroll_manager = scroll::ScrollController::new([0.0; 2]);

    //let foo=load_glb(BLOCK_GLB);
    let foo=load_glb(PERSON_GLB);

    let mut data=foo.gen();
    log!(format!("{:?}", &data.positions.len()));
    log!(format!("{:?}", &data.indices.len()));
    log!(format!("tex:{:?}", &data.tex_coords.len()));

    {
        use matrix::*;
        //for key
        //let s=matrix::scale(2.0,2.0,2.0).chain(matrix::x_rotation(PI/2.0)).generate();
        
        //for person
        let s=matrix::scale(200.0,200.0,200.0).chain(matrix::x_rotation(PI/2.0)).generate();
        
        for p in data.positions.iter_mut(){
            *p=transform_point_3d(&s,*p);
            
        }
    }
    let mut index=simple2d::IndexBuffer::new(&ctx).unwrap_throw();
    index.update(&data.indices);

    let mut tex_coord=simple2d::TextureCoordBuffer::new(&ctx).unwrap_throw();
    tex_coord.update(&data.tex_coords);

    let mut tex=simple2d::TextureBuffer::new(&ctx);
    tex.update(32,32,data.texture.unwrap_throw());

    'outer: loop {
        // let s=matrix::z_rotation(0.1).generate();
        
        // for p in data.positions.iter_mut(){
        //     *p=transform_point_3d(&s,*p);    
        // }

        let mut j = false;
        for e in frame_timer.next().await {
            //log!(format!("{:?}", e));
            match e {
                MEvent::CanvasMouseUp => {
                    if scroll_manager.handle_mouse_up() {
                        j = true;
                    }
                }
                MEvent::CanvasMouseMove { x, y } => {
                    scroll_manager.handle_mouse_move([*x, *y]);
                }
                MEvent::CanvasMouseDown { x, y } => {
                    scroll_manager.handle_mouse_down([*x, *y]);
                }
                MEvent::ButtonClick => {
                    let _ = color_iter.next();
                }
                MEvent::ShutdownClick => break 'outer,
            }
        }
        let mouse_world = mouse_to_world(scroll_manager.cursor_canvas, scroll_manager.camera());

        //log!(format!("{:?}", (scroll_manager.cursor_canvas,scroll_manager.camera(),mouse_world)));

        if j {
            grid_walls.set(grid_viewport.to_grid((mouse_world).into()), true);
            update_walls(&grid_viewport, cache, &mut walls, &grid_walls);
        }
        scroll_manager.step();

        use matrix::*;

        simple2d::shapes(cache).rect(simple2d::Rect {
            x: mouse_world[0] - grid_viewport.spacing / 2.0,
            y: mouse_world[1] - grid_viewport.spacing / 2.0,
            w: grid_viewport.spacing,
            h: grid_viewport.spacing,
        },0.0);

        buffer.update_clear(cache);




        ctx.draw_clear([0.0, 0.0, 0.0, 0.0]);

        let matrix = world_to_screen(scroll_manager.camera())
            .chain(screen_to_clip(viewport))
            .generate();

        let mut v = draw_sys.view(&matrix);

        // v.draw_triangles(&checker, None,&[0.3, 0.3, 0.3, 0.3]);
        // v.draw_triangles(&walls,None, &[1.0, 0.5, 0.5, 1.0]);
        // v.draw_triangles(&buffer,None, color_iter.peek().unwrap_throw());


        {
            
            buffer.update_no_clear(&data.positions);
            v.draw_triangles(&tex,&tex_coord,&buffer,Some(&index), color_iter.peek().unwrap_throw());
        }
        
        
        

        ctx.flush();
    }

    w.post_message(());

    log!("worker thread closing");
}

use shogo::simple2d::Vertex;

fn update_walls(
    grid_viewport: &duckduckgeo::grid::GridViewPort,
    cache: &mut Vec<Vertex>,
    buffer: &mut DynamicBuffer,
    grid_walls: &grid::Grid2D,
) {
    let mut s = simple2d::shapes(cache);
    for (p, val) in grid_walls.iter() {
        if val {
            let top_left = grid_viewport.to_world_topleft(p);
            s.rect(simple2d::Rect {
                x: top_left.x,
                y: top_left.y,
                w: grid_viewport.spacing,
                h: grid_viewport.spacing,
            },-0.1);
        }
    }
    buffer.update_clear(cache);
}

//convert DOM coordinate to canvas relative coordinate
fn convert_coord(canvas: &web_sys::HtmlElement, event: &web_sys::Event) -> [f32; 2] {
    use wasm_bindgen::JsCast;
    shogo::simple2d::convert_coord(canvas, event.dyn_ref().unwrap_throw())
}

fn convert_coord_touch(canvas: &web_sys::HtmlElement, event: &web_sys::Event) -> [f32; 2] {
    event.prevent_default();
    event.stop_propagation();
    use wasm_bindgen::JsCast;
    convert_coord_touch_inner(canvas, event.dyn_ref().unwrap_throw())[0]
}

///
/// Convert a mouse event to a coordinate for simple2d.
///
pub fn convert_coord_touch_inner(
    canvas: &web_sys::HtmlElement,
    e: &web_sys::TouchEvent,
) -> Vec<[f32; 2]> {
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

    let mut ans = vec![];
    for a in 0..touches.length() {
        let touch = touches.get(a).unwrap();
        let x = touch.client_x() as f64;
        let y = touch.client_y() as f64;
        let rx = touch.radius_x() as f64;
        let ry = touch.radius_y() as f64;
        //log!(format!("{:?}",(rx,ry)));
        let [x, y] = [
            (x + rx - rect.left()) * scalex,
            (y + ry - rect.top()) * scaley,
        ];

        ans.push([x as f32, y as f32]);
    }
    ans
}

use webgl_matrix::prelude::*;

fn transform_point_3d(matrix: &[f32; 16], point: [f32; 3]) -> [f32; 3] {
    fn to_vec4(j: [f32; 3]) -> [f32; 4] {
        [j[0], j[1], j[2], 1.0]
    }

    let res = matrix.mul_vector_left(&to_vec4(point));
    [res[0], res[1], res[2]]
}

fn mouse_to_world(mouse: [f32; 2], camera: [f32; 2]) -> [f32; 2] {
    use matrix::*;
    let k = world_to_screen(camera).inverse();

    let depth = mouse[1];

    let matrix = k.generate();

    let a = transform_point_3d(&matrix, [mouse[0], mouse[1], depth]);
    [a[0], a[1]]
}

fn screen_to_clip(dim: [f32; 2]) -> impl matrix::MyMatrix + matrix::Inverse {
    use matrix::*;
    //Deep enough that we can tilt the whole board and have it still show up
    let depth = dim[0] * dim[1];
    let d = scale(2.0 / dim[0], -2.0 / dim[1], -2.0 / depth);
    let e = translation(-1.0, 1.0, 0.0);

    d.chain(e)
}

fn world_to_screen(offset: [f32; 2]) -> impl matrix::MyMatrix + matrix::Inverse {
    use matrix::*;
    let a = z_rotation(std::f32::consts::PI / 4.);
    let b = x_rotation(std::f32::consts::PI / 4.);
    let c = translation(offset[0], offset[1], 0.0);
    a.chain(c).chain(b)
}



const KEY_GLB:&'static [u8]=include_bytes!("../assets/key.glb");
const PERSON_GLB:&'static [u8]=include_bytes!("../assets/person-v1.glb");



pub struct Doop{
    document:gltf::Document,
    buffers:Vec<gltf::buffer::Data>,
    images:Vec<gltf::image::Data>
}

//TODO wouldnt it be amazing if this was a const function????
fn load_glb(bytes:&[u8])->Doop{
    //Use https://www.gltfeditor.com/ also
    //Use https://gltf.report/ to compress it to the binary format!!!!
    
    //TODO discard normal verticies if not used???


    let (document,buffers,images)=gltf::import_slice(bytes).map_err(|e|log!(format!("{:?}", e))).unwrap_throw();
    log!(format!("{:?}", document));
    Doop { document, buffers, images }
}


struct ModelData<'a>{
    positions:Vec<[f32;3]>,
    indices:Vec<u16>,
    texture:Option<&'a [u8]>,
    tex_coords:Vec<[f32;2]>
}

impl Doop{

    fn gen(&self)->ModelData{
        // TODO use this: https://www.nayuki.io/page/png-file-chunk-inspector
        let mut positions=Vec::new();
        let mut indices=Vec::new();
        let mut offset=0;   
        let mut tex_coords=Vec::new();
        let texture=if let Some(texture)= self.document.textures().next(){

            log!("found a texture!");
            let g_img = texture.source();
            
            let buffers = &self.buffers;
            Some(match g_img.source() {
                Source::View { view, mime_type } => {
                    let parent_buffer_data = &buffers[view.buffer().index()].0;
                    let data = &parent_buffer_data[view.offset()..view.offset() + view.length()];
                    log!(format!("{:?}",data));
                    data
                },
                _=>{panic!("not supported")}
            })
        }else{
            None
        };


        for mesh in self.document.meshes(){
            
            for p in mesh.primitives(){
                //only support triangles
                assert_eq!(p.mode(),gltf::mesh::Mode::Triangles);
                

                let reader = p.reader(|buffer| Some(&self.buffers[buffer.index()]));

                let p:Vec<_>=reader.read_positions().unwrap_throw().collect();
                
                let i:Vec<_>=reader.read_indices().unwrap_throw().into_u32().map(|x|offset+(x as u16)).collect();
                

                
                if let Some(t) = reader.read_tex_coords(0) {
                    tex_coords.extend(t.into_f32())
                } else {
                    if texture.is_some(){
                        panic!("no texture coords!");
                    }
                };
                //log!(format!("pos:{:?}", &p));

                //log!(format!("ind:{:?}", &i));
                
                offset+=p.len() as u16;
                positions.extend(p);

                indices.extend(i);


            } 
        };
        
        ModelData { texture,positions,indices,tex_coords}
        
    }

}