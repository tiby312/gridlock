use cgmath::{InnerSpace, Vector2};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Touches {
    pub all: [(i32, f32, f32); 4],
    pub count: usize,
}
impl Touches {
    //TODO return reference
    pub fn get_pos(&self, a: i32) -> Option<[f32; 2]> {
        self.all
            .iter()
            .take(self.count)
            .find(|&b| b.0 == a)
            .map(|a| [a.1, a.2])
    }
    pub fn select_lowest_touch(&self) -> Option<i32> {
        self.all
            .iter()
            .take(self.count)
            .min_by_key(|a| a.0)
            .map(|a| a.0)
    }
    pub fn select_lowest_touch_excluding(&self, b: i32) -> Option<i32> {
        self.all
            .iter()
            .take(self.count)
            .filter(|a| a.0 != b)
            .min_by_key(|a| a.0)
            .map(|a| a.0)
    }
}

enum Foo {
    OneTouchActive {
        touch_id: i32,
    },
    TwoTouchActive {
        zoom: ZoomDelta,
        first_touch_id: i32,
        second_touch_id: i32,
    },
    MouseActive {
        canvas_pos: [f32; 2],
    },
    None,
}

#[derive(Copy, Clone)]
struct ZoomDelta {
    starting_distance: f32,
    current_distance: f32,
}

impl ZoomDelta {
    fn compute(&self) -> f32 {
        self.starting_distance - self.current_distance
    }
    fn update(&mut self, a: f32) {
        self.current_distance = a;
    }
}

pub struct TouchController {
    inner: ScrollController,
    foo: Foo,
    persistent_zoom: f32,
}

fn compute_middle(touches: &Touches, first: i32, second: i32) -> (f32, [f32; 2]) {
    let first_pos: Vector2<f32> = touches.get_pos(first).unwrap().into();
    let second_pos: Vector2<f32> = touches.get_pos(second).unwrap().into();
    let offset = second_pos - first_pos;
    let dis = offset.magnitude();
    let middle = first_pos + offset / 2.0;
    (dis, middle.into())
}

impl TouchController {
    pub fn new(camera: Vector2<f32>) -> Self {
        let inner = ScrollController::new(camera);
        TouchController {
            inner,
            foo: Foo::None,
            persistent_zoom: 0.0,
        }
    }

    pub fn on_mouse_down(&mut self, canvas_pos: [f32; 2]) {
        match self.foo {
            Foo::None => {
                self.inner.handle_mouse_down(canvas_pos);
                self.foo = Foo::MouseActive { canvas_pos }
            }
            _ => {}
        }
    }
    pub fn on_mouse_up(&mut self) -> MouseUp {
        match self.foo {
            Foo::MouseActive { .. } => {
                self.foo = Foo::None;
                self.inner.handle_mouse_up()
            }
            _ => MouseUp::NoSelect,
        }
    }
    pub fn on_mouse_move(&mut self, pos: [f32; 2], view_projection: ViewProjection) {
        match self.foo {
            Foo::MouseActive { .. } => {
                self.inner.handle_mouse_move(pos, view_projection);
            }
            _ => {}
        }
    }

    pub fn on_new_touch(&mut self, touches: &Touches) {
        match self.foo {
            Foo::OneTouchActive { touch_id } => {
                let second_touch_id = touches.select_lowest_touch_excluding(touch_id).unwrap();

                let (dis, middle) = compute_middle(&touches, touch_id, second_touch_id);

                //we don't want to propogate this click to the user.
                let _ = self.inner.handle_mouse_up();
                self.inner.handle_mouse_down(middle);

                self.foo = Foo::TwoTouchActive {
                    zoom: ZoomDelta {
                        starting_distance: dis,
                        current_distance: dis,
                    },

                    first_touch_id: touch_id,
                    second_touch_id,
                }
            }
            Foo::TwoTouchActive { .. } => {
                //ignore new touches. do nothing.
            }
            Foo::None => {
                //Guarenteed to exist because this function is called on new touch.
                let touch_id = touches.select_lowest_touch().unwrap();

                let mouse = touches.get_pos(touch_id).unwrap();
                self.inner.handle_mouse_down(mouse);
                //found one touch!
                //select one touch id.
                //find position
                self.foo = Foo::OneTouchActive { touch_id };
            }
            Foo::MouseActive { .. } => {
                //ignore touch mouse is active
            }
        }
    }

    pub fn on_touch_move(&mut self, touches: &Touches, view_projection: ViewProjection) {
        match self.foo {
            Foo::OneTouchActive { touch_id } => {
                let mouse = touches.get_pos(touch_id).unwrap();
                self.inner.handle_mouse_move(mouse, view_projection);
            }
            Foo::TwoTouchActive {
                mut zoom,
                first_touch_id,
                second_touch_id,
            } => {
                let (dis, middle) = compute_middle(&touches, first_touch_id, second_touch_id);
                self.inner.handle_mouse_move(middle, view_projection);
                zoom.update(dis);
                self.foo = Foo::TwoTouchActive {
                    zoom,
                    first_touch_id,
                    second_touch_id,
                }
            }
            Foo::None => {
                //A touch moved that we don't care about.
            }
            Foo::MouseActive { .. } => {
                //ignore touch mouse active
            }
        }
    }

    #[must_use]
    pub fn on_touch_up(&mut self, touches: &Touches) -> MouseUp {
        match self.foo {
            Foo::OneTouchActive { touch_id } => {
                if touches.get_pos(touch_id).is_none() {
                    self.foo = Foo::None;
                    self.inner.handle_mouse_up()
                } else {
                    MouseUp::NoSelect
                }
            }
            Foo::TwoTouchActive {
                zoom,
                first_touch_id,
                second_touch_id,
            } => {
                let a = touches.get_pos(first_touch_id);
                let b = touches.get_pos(second_touch_id);

                match (a, b) {
                    (None, None) => {
                        //two touches got removed simultaneously
                        //don't propograte. otherwise it would click in the middle of both touches.
                        let _ = self.inner.handle_mouse_up();
                        self.foo = Foo::None;
                        self.persistent_zoom += zoom.compute();
                        MouseUp::NoSelect
                    }
                    (None, Some(pos)) => {
                        let _ = self.inner.handle_mouse_up();
                        self.inner.handle_mouse_down(pos);
                        self.foo = Foo::OneTouchActive {
                            touch_id: second_touch_id,
                        };
                        self.persistent_zoom += zoom.compute();
                        MouseUp::NoSelect
                    }
                    (Some(pos), None) => {
                        let _ = self.inner.handle_mouse_up();
                        self.inner.handle_mouse_down(pos);
                        self.foo = Foo::OneTouchActive {
                            touch_id: first_touch_id,
                        };
                        self.persistent_zoom += zoom.compute();
                        MouseUp::NoSelect
                    }
                    (Some(_), Some(_)) => {
                        //A touch we don't care about went up.
                        MouseUp::NoSelect
                    }
                }
            }
            Foo::None => {
                //Touch up for a touch we don't care about.
                MouseUp::NoSelect
            }
            Foo::MouseActive { .. } => {
                //ignore touch mosue active
                MouseUp::NoSelect
            }
        }
    }

    pub fn step(&mut self) {
        self.inner.step();
    }

    pub fn zoom(&self) -> f32 {
        let z = if let Foo::TwoTouchActive { zoom, .. } = &self.foo {
            zoom.compute()
        } else {
            0.0
        };
        self.persistent_zoom + z
    }

    //camera in world coordinates
    pub fn camera(&self) -> [f32; 2] {
        self.inner.camera()
    }

    pub fn cursor_canvas(&self) -> [f32; 2] {
        self.inner.cursor_canvas()
    }
}

pub enum MouseUp {
    /// This was a select
    Select,
    /// This was a scroll mouse up
    NoSelect,
}

#[derive(PartialEq, Debug)]
enum Scrollin {
    MouseDown {
        mouse_anchor: Vector2<f32>,
        camera_anchor: Vector2<f32>,
    },
    Scrolling {
        mouse_anchor: Vector2<f32>,
        camera_anchor: Vector2<f32>,
    },
    NotScrolling,
}
use super::*;
pub struct ScrollController {
    //canvas coordinates
    cursor_canvas: Vector2<f32>,

    //world coord
    camera: Vector2<f32>,
    last_camera: Vector2<f32>,

    scrolling: Scrollin,
}

impl ScrollController {
    pub fn new(camera: Vector2<f32>) -> Self {
        ScrollController {
            camera,
            last_camera: camera.into(),
            cursor_canvas: [0.0; 2].into(),
            scrolling: Scrollin::NotScrolling,
        }
    }

    pub fn camera(&self) -> [f32; 2] {
        [self.camera[0], self.camera[1]]
    }

    pub fn handle_mouse_move(&mut self, mouse: [f32; 2], view_projection: ViewProjection) {
        self.cursor_canvas = mouse.into();

        match self.scrolling {
            Scrollin::Scrolling {
                mouse_anchor,
                camera_anchor,
            } => {
                let mouse_world1: Vector2<f32> =
                    mouse_to_world(self.cursor_canvas.into(), view_projection).into();

                let mouse_world2: Vector2<f32> =
                    mouse_to_world(mouse_anchor.into(), view_projection).into();

                let offset = mouse_world2 - mouse_world1;
                self.last_camera = self.camera;
                self.camera = camera_anchor + offset;
            }
            Scrollin::MouseDown {
                mouse_anchor,
                camera_anchor,
            } => {
                let a = Vector2::from(self.cursor_canvas);
                let b = Vector2::from(mouse_anchor);
                let offset = b - a;
                if offset.magnitude2() > 10.0 * 10.0 {
                    self.scrolling = Scrollin::Scrolling {
                        mouse_anchor,
                        camera_anchor,
                    }
                }
            }
            _ => {}
        }
    }

    pub fn handle_mouse_down(&mut self, mouse: [f32; 2]) {
        self.cursor_canvas = mouse.into();

        self.scrolling = Scrollin::MouseDown {
            mouse_anchor: Vector2::from(self.cursor_canvas),
            camera_anchor: Vector2::from(self.camera),
        };
    }

    //Return true if a regular tap is detected.
    //TODO replace with enum!
    #[must_use]
    pub fn handle_mouse_up(&mut self) -> MouseUp {
        match self.scrolling {
            Scrollin::MouseDown { .. } => {
                self.scrolling = Scrollin::NotScrolling;
                MouseUp::Select
            }
            Scrollin::Scrolling { .. } => {
                self.scrolling = Scrollin::NotScrolling;
                MouseUp::NoSelect
            }
            Scrollin::NotScrolling => {
                panic!("not possible?")
            }
        }
    }
    pub fn step(&mut self) {
        match self.scrolling {
            Scrollin::Scrolling { .. } => {}
            _ => {
                let delta = Vector2::from(self.camera) - Vector2::from(self.last_camera);
                self.last_camera = Vector2::from(self.camera);

                self.camera = (Vector2::from(self.camera) + delta * 0.9).into();
            }
        }
    }

    pub fn cursor_canvas(&self) -> [f32; 2] {
        self.cursor_canvas.into()
    }
}

pub fn mouse_to_world(mouse: [f32; 2], view_projection: ViewProjection) -> [f32; 2] {
    //generate some mouse points
    let clip_x = mouse[0] / view_projection.dim[0] * 2. - 1.;
    let clip_y = mouse[1] / view_projection.dim[1] * -2. + 1.;
    clip_to_world([clip_x, clip_y], view_projection)
}
