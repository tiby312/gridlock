use axgeom::vec2same;
use cgmath::{InnerSpace, Matrix4, Transform, Vector2};
use duckduckgeo::grid::Grid2D;
use futures::{FutureExt, SinkExt, StreamExt};
use gloo::console::log;
use model::matrix::{self, MyMatrix};
use movement::GridCoord;
use serde::{Deserialize, Serialize};
use shogo::simple2d::{self, ShaderSystem};
use shogo::utils;
use wasm_bindgen::prelude::*;
pub mod animation;
pub mod dom;
pub mod gameplay;
pub mod grids;
pub mod model_parse;
pub mod movement;
pub mod projection;
pub mod scroll;
pub mod terrain;
pub mod util;
use dom::MEvent;
use projection::*;
//pub mod logic;
pub const RESIZE: usize = 6;

#[derive(Serialize, Deserialize, Debug, Copy, Clone)]
enum UiButton {
    ShowRoadUi,
    NoUi,
}

pub struct WarriorDraw<'a> {
    model: &'a MyModel,
    drop_shadow: &'a MyModel,
    col: &'a UnitCollection<Warrior>,
}
impl<'a> WarriorDraw<'a> {
    fn new(col: &'a UnitCollection<Warrior>, model: &'a MyModel, drop_shadow: &'a MyModel) -> Self {
        Self {
            model,
            drop_shadow,
            col,
        }
    }
    fn draw(&self, gg: &grids::GridMatrix, draw_sys: &mut ShaderSystem, matrix: &Matrix4<f32>) {
        for cc in self.col.elem.iter() {
            let pos: [f32; 2] = gg.to_world_topleft(cc.position.0.into()).into();

            let t = matrix::translation(pos[0], pos[1], 20.0);
            let s = matrix::scale(1.0, 1.0, 1.0);
            let m = matrix.chain(t).chain(s).generate();
            let mut v = draw_sys.view(m.as_ref());

            self.model
                .draw_ext(&mut v, !cc.is_selectable(), false, false);
        }
    }

    fn draw_shadow(
        &self,
        gg: &grids::GridMatrix,
        draw_sys: &mut ShaderSystem,
        matrix: &Matrix4<f32>,
    ) {
        for &GridCoord(a) in self.col.elem.iter().map(|a| &a.position) {
            let pos: [f32; 2] = gg.to_world_topleft(a.into()).into();
            let t = matrix::translation(pos[0], pos[1], 1.0);

            let m = matrix.chain(t).generate();

            let mut v = draw_sys.view(m.as_ref());
            self.drop_shadow.draw(&mut v);
        }
    }

    fn draw_health_text(
        &self,
        gg: &grids::GridMatrix,
        health_numbers: &NumberTextManager,
        view_proj: &Matrix4<f32>,
        proj: &Matrix4<f32>,
        draw_sys: &mut ShaderSystem,
    ) {
        //draw text
        for ccat in self.col.elem.iter() {
            let pos: [f32; 2] = gg.to_world_topleft(ccat.position.0.into()).into();

            let t = matrix::translation(pos[0], pos[1] + 20.0, 20.0);

            let jj = view_proj.chain(t).generate();
            let jj: &[f32; 16] = jj.as_ref();
            let tt = matrix::translation(jj[12], jj[13], jj[14]);
            let new_proj = proj.clone().chain(tt);

            let s = matrix::scale(5.0, 5.0, 5.0);
            let m = new_proj.chain(s).generate();

            let nn = health_numbers.get_number(ccat.health);
            let mut v = draw_sys.view(m.as_ref());
            nn.draw_ext(&mut v, false, false, true);

            //nn.draw(ccat.health,&ctx,&text_texture,&mut draw_sys,&m);
        }
    }
}

#[derive(Debug)]
pub struct UnitCollection<T: HasPos> {
    elem: Vec<T>,
}

impl<T: HasPos> UnitCollection<T> {
    fn new(elem: Vec<T>) -> Self {
        UnitCollection { elem }
    }
    fn remove(&mut self, a: &GridCoord) -> T {
        let (i, _) = self
            .elem
            .iter()
            .enumerate()
            .find(|(_, b)| b.get_pos() == a)
            .unwrap();
        self.elem.swap_remove(i)
    }

    fn find_mut(&mut self, a: &GridCoord) -> Option<&mut T> {
        self.elem.iter_mut().find(|b| b.get_pos() == a)
    }
    fn find(&self, a: &GridCoord) -> Option<&T> {
        self.elem.iter().find(|b| b.get_pos() == a)
    }
    fn filter(&self) -> UnitCollectionFilter<T> {
        UnitCollectionFilter { a: &self.elem }
    }
}

pub struct SingleFilter<'a> {
    a: &'a GridCoord,
}
impl<'a> movement::Filter for SingleFilter<'a> {
    fn filter(&self, a: &GridCoord) -> bool {
        self.a != a
    }
}

pub struct UnitCollectionFilter<'a, T> {
    a: &'a [T],
}
impl<'a, T: HasPos> movement::Filter for UnitCollectionFilter<'a, T> {
    fn filter(&self, b: &GridCoord) -> bool {
        self.a.iter().find(|a| a.get_pos() == b).is_none()
    }
}

pub trait HasPos {
    fn get_pos(&self) -> &GridCoord;
}
impl HasPos for GridCoord {
    fn get_pos(&self) -> &GridCoord {
        self
    }
}

impl HasPos for Warrior {
    fn get_pos(&self) -> &GridCoord {
        &self.position
    }
}

type MyModel = model_parse::Foo<model_parse::TextureGpu, model_parse::ModelGpu>;

#[derive(Debug)]
pub struct Warrior {
    position: GridCoord,
    move_deficit: MoveUnit,
    moved: bool,
    health: i8,
}

impl Warrior {
    fn is_selectable(&self) -> bool {
        !self.moved
    }

    fn new(position: GridCoord) -> Self {
        Warrior {
            position,
            move_deficit: MoveUnit(0),
            moved: false,
            health: 10,
        }
    }
}

#[derive(Debug, Clone)]
pub enum CellSelection {
    MoveSelection(movement::PossibleMoves, movement::PossibleMoves),
    BuildSelection(GridCoord),
}

//TODO store actual world pos? Less calculation each iteration.
//Additionally removes need to special case animation.
pub struct Game {
    grid_matrix: grids::GridMatrix,
    //selected_cells: Option<CellSelection>,
    //animation: Option<animation::Animation<Warrior>>,
    dogs: UnitCollection<Warrior>,
    cats: UnitCollection<Warrior>,
}

#[wasm_bindgen]
pub async fn worker_entry() {
    console_error_panic_hook::set_once();

    let (mut w, ss) = shogo::EngineWorker::new().await;
    let mut frame_timer = shogo::FrameTimer::new(60, ss);

    let canvas = w.canvas();
    let ctx = simple2d::ctx_wrap(&utils::get_context_webgl2_offscreen(&canvas));

    let mut draw_sys = ctx.shader_system();
    let mut buffer = ctx.buffer_dynamic();
    let cache = &mut vec![];

    //TODO get rid of this somehow.
    //these values are incorrect.
    //they are set correctly after resize is called on startup.
    let gl_width = canvas.width(); // as f32*1.6;
    let gl_height = canvas.height(); // as f32*1.6;
    ctx.viewport(0, 0, gl_width as i32, gl_height as i32);
    let mut viewport = [canvas.width() as f32, canvas.height() as f32];

    ctx.setup_alpha();

    //TODO delete
    let gg = grids::GridMatrix::new();

    let mut scroll_manager = scroll::TouchController::new([0., 0.].into());

    let quick_load = |name| {
        let (data, t) = model::load_glb(name).gen_ext(gg.spacing(), RESIZE);
        model_parse::Foo {
            texture: model_parse::TextureGpu::new(&ctx, &t),
            model: model_parse::ModelGpu::new(&ctx, &data),
        }
    };

    let drop_shadow = quick_load(DROP_SHADOW_GLB);

    let dog = quick_load(DOG_GLB);

    let cat = quick_load(CAT_GLB);

    let road = quick_load(ROAD_GLB);

    let grass = quick_load(GRASS_GLB);

    let select_model = quick_load(SELECT_GLB);

    let attack_model = quick_load(ATTACK_GLB);

    let text_texture = {
        let ascii_tex = model::load_texture_from_data(include_bytes!("../assets/ascii5.png"));

        model_parse::TextureGpu::new(&ctx, &ascii_tex)
    };

    let health_numbers = NumberTextManager::new(0..=10, &ctx, &text_texture);

    let dogs = UnitCollection::new(vec![
        Warrior::new(GridCoord([3, 3])),
        Warrior::new(GridCoord([4, 4])),
    ]);

    let cats = UnitCollection::new(vec![
        Warrior::new(GridCoord([2, 2])),
        Warrior::new(GridCoord([5, 5])),
        Warrior::new(GridCoord([6, 6])),
        Warrior::new(GridCoord([7, 7])),
        Warrior::new(GridCoord([3, 1])),
    ]);

    //let selected_cells: Option<CellSelection> = None;

    // pub struct Doop {
    //     state: Game,
    //     select: Option<[f32; 2]>,
    // }
    let mut ga = Game {
        dogs,
        cats,
        //selected_cells,
        //animation,
        grid_matrix: grids::GridMatrix::new(),
    };

    //let (mut ggame, mut ggame2) = futures::lock::BiLock::new(&mut ga);
    let mut ggame = std::sync::Arc::new(futures::lock::Mutex::new(&mut ga));
    let ggame2 = ggame.clone();

    let mut roads = terrain::TerrainCollection {
        pos: vec![],
        func: |a: MoveUnit| MoveUnit(a.0 / 2),
    };

    use cgmath::SquareMatrix;
    let mut last_matrix = cgmath::Matrix4::identity();

    //let mut turn_counter = false;

    //let (mut tx, mut rx) = futures::channel::mpsc::channel(1);
    //let (mut animation_tx, mut animation_rx) = futures::channel::mpsc::channel(1);

    // let testy = async {
    //     for i in 0..5 {
    //         for j in 0..2 {
    //             // async fn handle_turn(){
    //             //     let cell=loop {
    //             //         let unit=select_team_unit().await;
    //             //         let area=generate_movement(selected_unit);
    //             //         let area_lock=send_draw_lock(area);
    //             //         if let Some(cell)=select_cell_from_area(area).await {
    //             //             break cell;
    //             //         }
    //             //     };

    //             //     move_to(unit,cell).await;

    //             //     {
    //             //         let area=generate_attack(selected_unit);
    //             //         let area_lock=send_draw_lock(area).await;
    //             //         if let Some(cell)=select_cell_from_area_with_enemy(area).await{
    //             //             attack_unit(unit,cell)
    //             //         }
    //             //     }
    //             //     //now move unit to cell
    //             // };

    //             loop {
    //                 let mut d = logic::Doop {
    //                     game: ggame2.clone(),
    //                     rx: &mut rx,
    //                     team: j,
    //                 };
    //                 //Wait for user to click a unit and present move options to user

    //                 let (mut game, cell) = d.get_possible_moves().await;
    //                 game.selected_cells = Some(cell);
    //                 drop(game);

    //                 if d.pick_possible_move().await {
    //                     break;
    //                 }
    //             }

    //             {
    //                 //Wait for the animation to finish and then show attack area
    //                 animation_rx.next().await;
    //                 let mut gg1 = ggame2.lock().await;
    //                 let gg = &mut *gg1;
    //                 let [this_team, that_team] = logic::team_view([&mut gg.cats, &mut gg.dogs], j);
    //                 let unit = gg.animation.take().unwrap().into_data();

    //                 this_team.elem.push(unit);
    //                 let unit = this_team.elem.last().unwrap();

    //                 gg.selected_cells = Some(get_cat_move_attack_matrix(
    //                     unit,
    //                     this_team.filter(),
    //                     roads.foo(),
    //                     &gg.grid_matrix,
    //                 ));
    //             }

    //             //Wait for user to select a valid attack cell, or no cell
    //             loop {
    //                 let mouse_world: [f32; 2] = rx.next().await.unwrap();
    //                 let mut gg1 = ggame2.lock().await;
    //                 let gg = &mut *gg1;
    //                 let [this_team, that_team] = logic::team_view([&mut gg.cats, &mut gg.dogs], j);

    //                 let cell: GridCoord =
    //                     GridCoord(gg.grid_matrix.to_grid((mouse_world).into()).into());

    //                 let s = gg.selected_cells.as_mut().unwrap();

    //                 match s {
    //                     CellSelection::MoveSelection(ss, attack) => {
    //                         let target_cat_pos = &cell;

    //                         if movement::contains_coord(attack.iter_coords(), target_cat_pos)
    //                             && that_team.find(target_cat_pos).is_some()
    //                         {
    //                             //attacking!
    //                             let target_cat = that_team.find_mut(target_cat_pos).unwrap();
    //                             target_cat.health -= 1;

    //                             let current_cat = this_team.find_mut(ss.start()).unwrap();
    //                             current_cat.moved = true;

    //                             gg.selected_cells = None;

    //                             //Finish user turn
    //                             break;
    //                         } else {
    //                             let current_cat = this_team.find_mut(ss.start()).unwrap();
    //                             current_cat.moved = true;

    //                             gg.selected_cells = None;
    //                             break;
    //                         }
    //                     }
    //                     _ => {
    //                         todo!()
    //                     }
    //                 }
    //             }
    //         }

    //         //log!(format!("got mouse pos:{:?}", &gg.dogs));
    //     }
    //     log!("DOOOONE");
    // }
    // .fuse();
    // futures::pin_mut!(testy);

    pub struct Doop {
        state: Game,
        select: Option<[f32; 2]>,
    }

    struct Doopo;
    impl gameplay::Zoo for Doopo {
        type G<'a> = Stuff<'a>;
        fn create() -> Self {
            Doopo
        }
    }
    struct Stuff<'a> {
        a: &'a mut Game,
        mouse: Option<[f32; 2]>,
    }

    let wait_mouse_input = || {
        //set cell

        gameplay::wait_custom(Doopo, |e| {
            //e.draw(c);
            if let Some(m) = e.mouse {
                gameplay::Stage::NextStage(m)
            } else {
                gameplay::Stage::Stay
            }
        })
    };

    // let animate = || {
    //     let mut animator = 0;
    //     gameplay::wait_custom(Doopo, move |e| {
    //         animator += 1;
    //         if animator > 30 {
    //             gameplay::Stage::NextStage(())
    //         } else {
    //             gameplay::Stage::Stay
    //         }
    //     })
    // };

    pub struct AnimationTicker {
        a: animation::Animation<Warrior>,
    }
    impl AnimationTicker {
        pub fn new(a: animation::Animation<Warrior>) -> Self {
            Self { a }
        }
    }
    impl GameStepper<Doopo> for AnimationTicker {
        type Result = gameplay::Next;
        fn step(&mut self, game: &mut Stuff<'_>) -> gameplay::Stage<Self::Result> {
            if let Some(_) = self.a.animate_step() {
                gameplay::Stage::Stay
            } else {
                gameplay::Stage::NextStage(gameplay::next())
            }
        }

        fn get_animation(&self) -> Option<&crate::animation::Animation<Warrior>> {
            Some(&self.a)
        }
    }

    pub struct PlayerCellAsk {
        a: Option<CellSelection>,
        team: usize,
    }

    impl PlayerCellAsk {
        pub fn new(a: CellSelection, team: usize) -> Self {
            Self { a: Some(a), team }
        }
    }
    impl GameStepper<Doopo> for PlayerCellAsk {
        type Result = (CellSelection, Option<GridCoord>);
        fn get_selection(&self) -> Option<&CellSelection> {
            self.a.as_ref()
        }
        fn step(&mut self, g1: &mut Stuff<'_>) -> gameplay::Stage<Self::Result> {
            let game = &mut g1.a;
            if let Some(mouse_world) = g1.mouse {
                let [this_team, that_team] =
                    gameplay::team_view([&mut game.cats, &mut game.dogs], self.team);

                let cell: GridCoord =
                    GridCoord(game.grid_matrix.to_grid((mouse_world).into()).into());

                match &self.a {
                    Some(CellSelection::MoveSelection(ss, attack)) => {
                        let target_cat_pos = &cell;

                        if movement::contains_coord(ss.iter_coords(), &cell) {
                            // let mut c = this_team.remove(ss.start());
                            // let (dd, aa) = ss.get_path_data(cell).unwrap();
                            // c.position = cell;
                            // c.move_deficit = *aa;
                            // //c.moved = true;

                            // let a = animation::Animation::new(ss.start(), dd, &game.grid_matrix, c);

                            gameplay::Stage::NextStage((self.a.take().unwrap(), Some(cell)))
                        } else {
                            gameplay::Stage::NextStage((self.a.take().unwrap(), None))
                        }
                    }
                    _ => {
                        todo!()
                    }
                }
            } else {
                gameplay::Stage::Stay
            }
        }
    }
    // let player_move_select = move |a: CellSelection, team: usize| {
    //     //TODO get rid of
    //     let kk = a.clone();
    //     gameplay::once(Doopo, move |g| {
    //         g.a.selected_cells = Some(kk);
    //         wait_mouse_input()
    //     })
    //     .and_then(move |mouse_world, g| {
    //         let game = &mut g.a;
    //         let [this_team, that_team] =
    //             gameplay::team_view([&mut game.cats, &mut game.dogs], team);

    //         let cell: GridCoord = GridCoord(game.grid_matrix.to_grid((mouse_world).into()).into());

    //         match &a {
    //             CellSelection::MoveSelection(ss, attack) => {
    //                 let target_cat_pos = &cell;

    //                 if movement::contains_coord(ss.iter_coords(), &cell) {
    //                     let mut c = this_team.remove(ss.start());
    //                     let (dd, aa) = ss.get_path_data(cell).unwrap();
    //                     c.position = cell;
    //                     c.move_deficit = *aa;
    //                     //c.moved = true;

    //                     let a = animation::Animation::new(ss.start(), dd, &game.grid_matrix, c);
    //                     game.selected_cells = None;
    //                     gameplay::optional(Some(AnimationTicker::new(a)))
    //                 } else {
    //                     game.selected_cells = None;
    //                     gameplay::optional(None)
    //                 }
    //             }
    //             _ => {
    //                 todo!()
    //             }
    //         }
    //     })
    // };

    let select_unit = move |team| {
        gameplay::looper2(wait_mouse_input(), move |mouse_world, stuff| {
            let game = &mut stuff.a;
            let [this_team, that_team] =
                gameplay::team_view([&mut game.cats, &mut game.dogs], team);

            let cell: GridCoord = GridCoord(game.grid_matrix.to_grid((mouse_world).into()).into());

            let Some(unit)=this_team.find(&cell) else {
                return gameplay::LooperRes::Loop(wait_mouse_input());
            };

            if !unit.is_selectable() {
                return gameplay::LooperRes::Loop(wait_mouse_input());
            }

            let pos = get_cat_move_attack_matrix(
                unit,
                this_team.filter().chain(that_team.filter()),
                terrain::Grass,
                &game.grid_matrix,
            );

            gameplay::LooperRes::Finish(pos) //player_move_select(pos,team)
        })
    };

    let handle_move = move |team| {
        let k = move |team| {
            select_unit(team)
                .and_then(move |c, game| PlayerCellAsk::new(c, team))
                .and_then(move |(c, cell), g1| {
                    let game = &mut g1.a;
                    if let Some(cell) = cell {
                        let [this_team, that_team] =
                            gameplay::team_view([&mut game.cats, &mut game.dogs], team);

                        match c {
                            CellSelection::MoveSelection(ss, attack) => {
                                let mut c = this_team.remove(ss.start());
                                let (dd, aa) = ss.get_path_data(cell).unwrap();
                                c.position = cell;
                                c.move_deficit = *aa;
                                //c.moved = true;
                                let aa =
                                    animation::Animation::new(ss.start(), dd, &game.grid_matrix, c);
                                let aaa = AnimationTicker::new(aa);
                                gameplay::optional(Some(aaa))
                            }
                            CellSelection::BuildSelection(_) => todo!(),
                        }
                    } else {
                        gameplay::optional(None)
                    }
                })
        };

        gameplay::looper2(k(team), move |res, stuff| match res {
            Some(animation) => gameplay::LooperRes::Finish(gameplay::next()),
            None => gameplay::LooperRes::Loop(k(team)),
        })
    };

    let mut testo = handle_move(0);

    // //TODO use this!
    // let mut cc = 0;
    // let mut k = gameplay::looper(Doopo, |_| {
    //     cc += 1;
    //     if cc > 2 {
    //         None
    //     } else {
    //         Some(player_turn(0).and_then(|w, g| player_turn(1)))
    //     }
    // })
    // .and_then(|_, _| {
    //     log!("completely done!");
    //     gameplay::next()
    // });

    'outer: loop {
        let mut on_select = false;

        let res = frame_timer.next().await;

        // let res = futures::select! {
        //     foo=frame_timer.next().fuse()=>foo,
        //     _=testy=>continue
        // };

        let mut ggame = &mut *ggame.lock().await;

        //let res = frame_timer.next().await;
        let mut reset = false;
        for e in res {
            match e {
                MEvent::Resize {
                    canvasx: _canvasx,
                    canvasy: _canvasy,
                    x,
                    y,
                } => {
                    let xx = *x as u32;
                    let yy = *y as u32;
                    canvas.set_width(xx);
                    canvas.set_height(yy);
                    ctx.viewport(0, 0, xx as i32, yy as i32);

                    viewport = [xx as f32, yy as f32];
                    log!(format!("updating viewport to be:{:?}", viewport));
                }
                MEvent::TouchMove { touches } => {
                    scroll_manager.on_touch_move(touches, &last_matrix, viewport);
                }
                MEvent::TouchDown { touches } => {
                    //log!(format!("touch down:{:?}",touches));
                    scroll_manager.on_new_touch(touches);
                }
                MEvent::TouchEnd { touches } => {
                    //log!(format!("touch end:{:?}",touches));
                    if let scroll::MouseUp::Select = scroll_manager.on_touch_up(&touches) {
                        on_select = true;
                    }
                }
                MEvent::CanvasMouseLeave => {
                    log!("mouse leaving!");
                    let _ = scroll_manager.on_mouse_up();
                }
                MEvent::CanvasMouseUp => {
                    if let scroll::MouseUp::Select = scroll_manager.on_mouse_up() {
                        on_select = true;
                    }
                }
                MEvent::CanvasMouseMove { x, y } => {
                    //log!(format!("{:?}",(x,y)));

                    scroll_manager.on_mouse_move([*x, *y], &last_matrix, viewport);
                }
                MEvent::EndTurn => {
                    reset = true;
                }
                MEvent::CanvasMouseDown { x, y } => {
                    //log!(format!("{:?}",(x,y)));

                    scroll_manager.on_mouse_down([*x, *y]);
                }
                MEvent::ButtonClick => {
                    //     match ggame.selected_cells {
                    //     Some(CellSelection::BuildSelection(g)) => {
                    //         log!("adding to roads!!!!!");
                    //         //roads.pos.push(g);
                    //         ggame.selected_cells = None;
                    //     }
                    //     _ => {
                    //         panic!("Received button push when we did not ask for it!")
                    //     }
                    // }
                }
                MEvent::ShutdownClick => break 'outer,
            }
        }

        let proj = projection::projection(viewport).generate();
        let view_proj = projection::view_matrix(
            scroll_manager.camera(),
            scroll_manager.zoom(),
            scroll_manager.rot(),
        );

        let matrix = proj.chain(view_proj).generate();

        last_matrix = matrix;

        let mouse_world = scroll::mouse_to_world(scroll_manager.cursor_canvas(), &matrix, viewport);

        // if on_select {
        //     tx.try_send(mouse_world).unwrap();
        // }
        {
            let mouse = on_select.then_some(mouse_world);
            let mut jj = Stuff {
                a: &mut ggame,
                mouse,
            };
            testo.step(&mut jj);
        }

        scroll_manager.step();

        use matrix::*;

        // simple2d::shapes(cache).rect(
        //     simple2d::Rect {
        //         x: mouse_world[0] - grid_viewport.spacing / 2.0,
        //         y: mouse_world[1] - grid_viewport.spacing / 2.0,
        //         w: grid_viewport.spacing,
        //         h: grid_viewport.spacing,
        //     },
        //     mouse_world[2] - 10.0,
        // );

        buffer.update_clear(cache);

        ctx.draw_clear([0.0, 0.0, 0.0, 0.0]);

        let [vvx, vvy] = get_world_rect(&matrix, &gg);

        for a in (vvx[0]..vvx[1])
            .skip_while(|&a| a < 0)
            .take_while(|&a| a < gg.num_rows())
        {
            //both should be skip
            for b in (vvy[0]..vvy[1])
                .skip_while(|&a| a < 0)
                .take_while(|&a| a < gg.num_rows())
            {
                use matrix::*;
                let x1 = gg.spacing() * a as f32;
                let y1 = gg.spacing() * b as f32;
                let s = 0.99;
                let mm = matrix
                    .chain(translation(x1, y1, -1.0))
                    .chain(scale(s, s, s))
                    .generate();

                let mut v = draw_sys.view(mm.as_ref());
                grass.draw(&mut v);
            }
        }

        let cat_draw = WarriorDraw::new(&ggame.cats, &cat, &drop_shadow);
        let dog_draw = WarriorDraw::new(&ggame.dogs, &dog, &drop_shadow);

        disable_depth(&ctx, || {
            if let Some(a) = testo.get_selection() {
                match a {
                    CellSelection::MoveSelection(a, attack) => {
                        for GridCoord(a) in a.iter_coords() {
                            let pos: [f32; 2] = gg.to_world_topleft(a.into()).into();
                            let t = matrix::translation(pos[0], pos[1], 0.0);

                            let m = matrix.chain(t).generate();

                            let mut v = draw_sys.view(m.as_ref());
                            select_model.draw(&mut v);
                        }

                        for GridCoord(a) in attack.iter_coords() {
                            let pos: [f32; 2] = gg.to_world_topleft(a.into()).into();
                            let t = matrix::translation(pos[0], pos[1], 0.0);

                            let m = matrix.chain(t).generate();

                            let mut v = draw_sys.view(m.as_ref());
                            attack_model.draw(&mut v);
                        }
                    }
                    CellSelection::BuildSelection(_) => {}
                }
            }

            for GridCoord(a) in roads.pos.iter() {
                let pos: [f32; 2] = gg.to_world_topleft(a.into()).into();
                let t = matrix::translation(pos[0], pos[1], 3.0);

                let m = matrix.chain(t).generate();

                let mut v = draw_sys.view(m.as_ref());
                road.draw(&mut v);
            }
        });

        disable_depth(&ctx, || {
            //draw dropshadow

            cat_draw.draw_shadow(&gg, &mut draw_sys, &matrix);
            dog_draw.draw_shadow(&gg, &mut draw_sys, &matrix);

            if let Some(a) = &testo.get_animation() {
                let pos = a.calc_pos();
                let t = matrix::translation(pos[0], pos[1], 1.0);

                let m = matrix.chain(t).generate();

                let mut v = draw_sys.view(m.as_ref());
                drop_shadow.draw(&mut v);
            }
        });

        if let Some(a) = &testo.get_animation() {
            log!("animatingggg");
            let pos = a.calc_pos();
            let t = matrix::translation(pos[0], pos[1], 20.0);
            let s = matrix::scale(1.0, 1.0, 1.0);
            let m = matrix.chain(t).chain(s).generate();
            let mut v = draw_sys.view(m.as_ref());
            cat.draw(&mut v);
        }

        cat_draw.draw(&gg, &mut draw_sys, &matrix);
        dog_draw.draw(&gg, &mut draw_sys, &matrix);

        disable_depth(&ctx, || {
            cat_draw.draw_health_text(&gg, &health_numbers, &view_proj, &proj, &mut draw_sys);
            dog_draw.draw_health_text(&gg, &health_numbers, &view_proj, &proj, &mut draw_sys);
        });

        ctx.flush();
    }

    w.post_message(UiButton::NoUi);

    log!("worker thread closing");
}

fn disable_depth(ctx: &WebGl2RenderingContext, func: impl FnOnce()) {
    ctx.disable(WebGl2RenderingContext::DEPTH_TEST);
    ctx.disable(WebGl2RenderingContext::CULL_FACE);

    func();

    ctx.enable(WebGl2RenderingContext::DEPTH_TEST);
    ctx.enable(WebGl2RenderingContext::CULL_FACE);
}

fn get_cat_move_attack_matrix(
    cat: &Warrior,
    cat_filter: impl Filter,
    roads: impl MoveCost,
    gg: &grids::GridMatrix,
) -> CellSelection {
    let mm = if cat.moved {
        MoveUnit(0)
    } else {
        MoveUnit(6 - 1)
    };

    let mm = movement::PossibleMoves::new(
        &movement::WarriorMovement,
        &gg.filter().chain(cat_filter),
        &terrain::Grass.chain(roads),
        cat.position,
        mm,
    );

    let attack_range = 2 - 1;
    let attack = movement::PossibleMoves::new(
        &movement::WarriorMovement,
        &gg.filter().chain(SingleFilter { a: cat.get_pos() }),
        &terrain::Grass,
        cat.position,
        MoveUnit(attack_range),
    );

    CellSelection::MoveSelection(mm, attack)
}

//TODO just use reference???
fn string_to_coords<'a>(st: &str) -> model::ModelData {
    let num_rows = 16;
    let num_columns = 16;

    let mut tex_coords = vec![];
    let mut counter = 0.0;
    let dd = 20.0;
    let mut positions = vec![];

    let mut inds = vec![];
    for (_, a) in st.chars().enumerate() {
        let ascii = a as u8;
        let index = (ascii - 0/*32*/) as u16;

        //log!(format!("aaaa:{:?}",index));
        let x = (index % num_rows) as f32 / num_rows as f32;
        let y = (index / num_rows) as f32 / num_columns as f32;

        let x1 = x;
        let x2 = x1 + 1.0 / num_rows as f32;

        let y1 = y;
        let y2 = y + 1.0 / num_columns as f32;

        let a = [[x1, y1], [x2, y1], [x1, y2], [x2, y2]];

        tex_coords.extend(a);

        let iii = [0u16, 1, 2, 2, 1, 3].map(|a| positions.len() as u16 + a);

        let xx1 = counter;
        let xx2 = counter + dd;
        let yy1 = dd;
        let yy2 = 0.0;

        let zz = 0.0;
        let y = [
            [xx1, yy1, zz],
            [xx2, yy1, zz],
            [xx1, yy2, zz],
            [xx2, yy2, zz],
        ];

        positions.extend(y);

        inds.extend(iii);

        assert!(ascii >= 32);
        counter += dd;
    }

    let normals = positions.iter().map(|_| [0.0, 0.0, 1.0]).collect();

    let cc = 1.0 / dd;
    let mm = matrix::scale(cc, cc, cc).generate();

    let positions = positions
        .into_iter()
        .map(|a| mm.transform_point(a.into()).into())
        .collect();

    model::ModelData {
        positions,
        tex_coords,
        indices: Some(inds),
        normals,
        matrix: mm,
    }
}

use web_sys::WebGl2RenderingContext;

use crate::gameplay::GameStepper;
use crate::movement::{Filter, MoveUnit};
use crate::terrain::MoveCost;

const SELECT_GLB: &'static [u8] = include_bytes!("../assets/select_model.glb");
const DROP_SHADOW_GLB: &'static [u8] = include_bytes!("../assets/drop_shadow.glb");
const ROAD_GLB: &'static [u8] = include_bytes!("../assets/road.glb");
const ATTACK_GLB: &'static [u8] = include_bytes!("../assets/attack.glb");

// const SHADED_GLB: &'static [u8] = include_bytes!("../assets/shaded.glb");
// const KEY_GLB: &'static [u8] = include_bytes!("../assets/key.glb");
// const PERSON_GLB: &'static [u8] = include_bytes!("../assets/person-v1.glb");
const CAT_GLB: &'static [u8] = include_bytes!("../assets/tiger2.glb");
const DOG_GLB: &'static [u8] = include_bytes!("../assets/cat2.glb");

const GRASS_GLB: &'static [u8] = include_bytes!("../assets/grass.glb");

pub struct NumberTextManager<'a> {
    numbers: Vec<model_parse::ModelGpu>,
    texture: &'a model_parse::TextureGpu,
}
impl<'a> NumberTextManager<'a> {
    fn new(
        range: impl IntoIterator<Item = i8>,
        ctx: &WebGl2RenderingContext,
        texture: &'a model_parse::TextureGpu,
    ) -> Self {
        fn generate_number(number: i8, ctx: &WebGl2RenderingContext) -> model_parse::ModelGpu {
            let data = string_to_coords(&format!("{}", number));
            model_parse::ModelGpu::new(ctx, &data)
        }

        let numbers = range.into_iter().map(|i| generate_number(i, ctx)).collect();
        Self { numbers, texture }
    }

    fn get_number(
        &self,
        num: i8,
    ) -> model_parse::Foo<&model_parse::TextureGpu, &model_parse::ModelGpu> {
        let gpu = &self.numbers[num as usize];

        model_parse::Foo {
            texture: &self.texture,
            model: gpu,
        }
    }
}
