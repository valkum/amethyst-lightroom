//! Displays spheres with physically based materials.
#[allow(dead_code, unused_imports)]
use amethyst::{
    animation::{
        get_animation_set, AnimationBundle, AnimationCommand, AnimationControlSet, AnimationSet,
        EndControl, VertexSkinningBundle,
    },
    assets::{
        AssetLoaderSystemData, AssetStorage, Completion, HotReloadBundle, Handle, Loader, PrefabLoader,
        PrefabLoaderSystemDesc, ProgressCounter, RonFormat, Prefab
    },
    controls::{FlyControlBundle, FlyControlTag, FlyMovementSystemDesc, MouseFocusUpdateSystemDesc, CursorHideSystemDesc, HideCursor, WindowFocus },
    core::{
        ecs::{
            Component, DenseVecStorage, DispatcherBuilder, Entities, Entity, Join, Read,
            ReadStorage, System, SystemData, World, Write, WriteStorage,
        },
        shrev::{EventChannel, ReaderId},
        math::{Unit, UnitQuaternion, Quaternion, Vector3, U1, U3},
        Time, Transform, TransformBundle, SystemDesc, Parent
    },
    error::Error,
    gltf::GltfSceneLoaderSystemDesc,
    input::{
        is_close_requested, is_key_down, is_key_up, Axis, Bindings, Button, InputBundle, InputEvent,
        StringBindings,
    },
    prelude::*,
    renderer::{
        bundle::{RenderPlan, RenderPlugin},
        debug_drawing::DebugLines,
        light::{self, Light, PointLight},
        palette::{LinSrgba, Srgb, Srgba},
        rendy::{
            mesh::{Normal, Position, Tangent, TexCoord},
            texture::palette::load_from_linear_rgba,
        },
        resources::Tint,
        shape::Shape,
        types::{DefaultBackend, Mesh, Texture},
        visibility::BoundingSphere,
        ActiveCamera, Camera, Factory, ImageFormat, Material, MaterialDefaults, RenderDebugLines,
        RenderFlat2D, RenderFlat3D, RenderPbr3D, RenderShaded3D, RenderSkybox, RenderToWindow,
        RenderingBundle, SpriteRender, SpriteSheet, SpriteSheetFormat, Transparent,
    },
    utils::{
        application_root_dir,
        auto_fov::{AutoFov, AutoFovSystem},
        fps_counter::FpsCounterBundle,
        tag::TagFinder,
    },
};
use std::vec::Vec;
use amethyst::winit::{self, Event, DeviceEvent, WindowEvent, ElementState, MouseButton};
use amethyst_imgui::RenderImgui;
use std::path::Path;
use std::collections::HashMap;
use amethyst_derive::SystemDesc;
use derive_new::new;

// use amethyst_inspector::{Inspector, InspectorHierarchy, inspector};

use prefab_data::{AnimationMarker, Scene, ScenePrefabData, SpriteAnimationId};
use filtered_input::{FilterInputSystemDesc, FilteredInputEvent};
#[cfg(feature = "profiler")]
use thread_profiler::profile_scope;

mod prefab_data;
mod filtered_input;

struct Lightroom {
    initialised: bool,
    progress: Option<ProgressCounter>,
    scene: usize,
    scene_root: Option<Entity>
}

impl Lightroom {
    pub fn new(scene: usize) -> Self {
        Self {
            initialised: false,
            progress: None,
            scene,
            scene_root: None
        }
    }
}

type SceneMap = HashMap<usize, Handle<Prefab<ScenePrefabData>>>;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum RenderMode {
    Flat,
    Shaded,
    Pbr,
}

impl Default for RenderMode {
    fn default() -> Self {
        RenderMode::Pbr
    }
}

impl SimpleState for Lightroom {
    fn on_start(&mut self, data: StateData<'_, GameData<'_, '_>>) {
        #[cfg(feature = "profiler")]
        profile_scope!("example on_start");


        

        let StateData { world, .. } = data;

        let mat_defaults = world.read_resource::<MaterialDefaults>().0.clone();
        world.insert(SceneMap::new());
        world.insert(UIState::default());

        self.progress = Some(ProgressCounter::default());

        world.exec(
            |(loader, mut scene, mut scene_map): (PrefabLoader<'_, ScenePrefabData>, Write<'_, Scene>, Write<'_, SceneMap>)| {
                scene_map.insert(0,
                    loader.load(
                        Path::new("prefab")
                            .join("lightroom_0.ron")
                            .to_string_lossy(),
                        RonFormat,
                        self.progress.as_mut().unwrap(),
                    ),
                );
                scene_map.insert(1,
                    loader.load(
                        Path::new("prefab")
                            .join("lightroom_1.ron")
                            .to_string_lossy(),
                        RonFormat,
                        self.progress.as_mut().unwrap(),
                    ),
                );

                scene.handle = Some(scene_map.get(&0).unwrap().clone());
            },
        );
        

        // Create the camera
        let mut transform = Transform::default();
        transform.set_translation_xyz(0.0, 2.0, 4.0);

        let mut auto_fov = AutoFov::default();
        auto_fov.set_base_fovx(std::f32::consts::FRAC_PI_3);
        auto_fov.set_base_aspect_ratio(1, 1);

        let camera = world
            .create_entity()
            .with(Camera::standard_3d(16.0, 9.0))
            .with(auto_fov)
            .with(transform)
            .with(FlyControlTag)
            .build();

        world.insert(ActiveCamera {
            entity: Some(camera),
        });
        world.insert(RenderMode::default());
    }

    fn handle_event(
        &mut self,
        data: StateData<'_, GameData<'_, '_>>,
        event: StateEvent,
    ) -> SimpleTrans {
        #[cfg(feature = "profiler")]
        profile_scope!("example handle_event");
        let StateData { mut world, .. } = data;
        if let StateEvent::Window(event) = &event {
            if is_close_requested(&event) || is_key_down(&event, winit::VirtualKeyCode::Escape) {
                Trans::Quit
            } else if is_key_down(&event, winit::VirtualKeyCode::E) {
                let mut mode = world.write_resource::<RenderMode>();
                *mode = match *mode {
                    RenderMode::Flat => RenderMode::Shaded,
                    RenderMode::Shaded => RenderMode::Pbr,
                    RenderMode::Pbr => RenderMode::Flat,
                };
                Trans::None
            } else {
                Trans::None
            }
        } else {
            Trans::None
        }
    }

    fn update(&mut self, data: &mut StateData<'_, GameData<'_, '_>>) -> SimpleTrans {
        #[cfg(feature = "profiler")]
        profile_scope!("example update");

        if !self.initialised {
            let remove = match self.progress.as_ref().map(|p| p.complete()) {
                None | Some(Completion::Loading) => false,

                Some(Completion::Complete) => {
                    // let scene_handle = data
                    //     .world
                    //     .read_resource::<Scene>()
                    //     .handle
                    //     .as_ref()
                    //     .unwrap()
                    //     .clone();
                    println!("Loading of {} complete.", self.scene);
                    // self.scene_root = Some(data.world.create_entity().with(scene_handle).build());
                    true
                }

                Some(Completion::Failed) => {
                    println!("Error: {:?}", self.progress.as_ref().unwrap().errors());
                    return Trans::Quit;
                }
            };
            if remove {
                self.progress = None;
            }
        }
        
        Trans::None
    }
}

// This is required because rustc does not recognize .ctor segments when considering which symbols
// to include when linking static libraries, so we need to reference a symbol in each module that
// registers an importer since it uses inventory::submit and the .ctor linkage hack.
fn init_modules() {
    {
        use amethyst::assets::{Format, Prefab};
        let _w = amethyst::audio::output::outputs();
        let _p = Prefab::<()>::new();
        let _name = ImageFormat::default().name();
    }
}

fn main() -> amethyst::Result<()> {
    amethyst::Logger::from_config(amethyst::LoggerConfig {
        stdout: amethyst::StdoutLog::Off,
        log_file: Some("rendy_example.log".into()),
        level_filter: log::LevelFilter::Error,
        ..Default::default()
    })
    // .level_for("amethyst_utils::fps_counter", log::LevelFilter::Debug)
    // .level_for("rendy_memory", log::LevelFilter::Trace)
    // .level_for("rendy_factory", log::LevelFilter::Trace)
    // .level_for("rendy_resource", log::LevelFilter::Trace)
    // .level_for("rendy_graph", log::LevelFilter::Trace)
    // .level_for("rendy_node", log::LevelFilter::Trace)
    // .level_for("amethyst_rendy", log::LevelFilter::Trace)
    // .level_for("gfx_backend_metal", log::LevelFilter::Trace)
    .start();

    init_modules();

    let app_root = application_root_dir()?;
    let assets_dir = app_root.join("resources");

    let display_config_path = assets_dir
        .join("config")
        .join("display.ron");

    let mut bindings = Bindings::new();
    bindings.insert_axis(
        "vertical",
        Axis::Emulated {
            pos: Button::Key(winit::VirtualKeyCode::S),
            neg: Button::Key(winit::VirtualKeyCode::W),
        },
    )?;
    bindings.insert_axis(
        "horizontal",
        Axis::Emulated {
            pos: Button::Key(winit::VirtualKeyCode::D),
            neg: Button::Key(winit::VirtualKeyCode::A),
        },
    )?;
    bindings.insert_axis(
        "horizontal",
        Axis::Emulated {
            pos: Button::Key(winit::VirtualKeyCode::D),
            neg: Button::Key(winit::VirtualKeyCode::A),
        },
    )?;

    use renderdoc::{RenderDoc, V100, V120};
        let mut rd: renderdoc::RenderDoc<renderdoc::V120> =
        renderdoc::RenderDoc::new().expect("Failed to init renderdoc");

    let game_data = GameDataBuilder::default()
        .with(AutoFovSystem::default(), "auto_fov", &[])
        .with_bundle(FpsCounterBundle::default())?
        .with_system_desc(
            PrefabLoaderSystemDesc::<ScenePrefabData>::default(),
            "scene_loader",
            &[],
        )
        .with_system_desc(
            GltfSceneLoaderSystemDesc::default(),
            "gltf_loader",
            &["scene_loader"], // This is important so that entity instantiation is performed in a single frame.
        )
        .with_bundle(InputBundle::<StringBindings>::new().with_bindings(bindings))?
        .with_system_desc(FilterInputSystemDesc::default(), "input_filter", &["input_system"])
        .with_bundle(HotReloadBundle::default())?
        // .with_bundle(
        //     FlyControlBundle::<StringBindings>::new(
        //         Some("horizontal".into()),
        //         None,
        //         Some("vertical".into()),
        //     )
        //     .with_sensitivity(0.1, 0.1)
        //     .with_speed(5.),
        // )?
        .with_system_desc(FlyMovementSystemDesc::<StringBindings>::new(
                5.,
                Some("horizontal".into()),
                None,
                Some("vertical".into()),
            ),
            "fly_movement",
            &[],
        )
        .with_system_desc(CustomFreeRotationSystemDesc::new(0.1, 0.1, false),
            "free_rotation",
            &["input_filter"],
        )
        .with_system_desc(MouseFocusUpdateSystemDesc::default(),
            "mouse_focus",
            &["free_rotation"],
        )
        .with_system_desc(CursorHideSystemDesc::default(),
            "cursor_hide",
            &["mouse_focus"],
        )
        .with_system_desc(SceneChangeSystemDesc::default(), "scene_change", &[])
        .with_bundle(TransformBundle::new().with_dep(&[
            "fly_movement",
        ]))?
        .with_bundle(VertexSkinningBundle::new().with_dep(&[
            "transform_system",
        ]))?
        // .with(amethyst_inspector::InspectorHierarchy::<UserData>::default(), "", &[])
	    // .with(Inspector, "", &[""])
        .with(UISystem::default(), "imgui_use", &[])
        .with_bundle(
            RenderingBundle::<DefaultBackend>::new()
                .with_plugin(
                    RenderToWindow::from_config_path(display_config_path)?
                    .with_clear([0.34, 0.36, 0.52, 1.0]),
                )
                .with_plugin(RenderSwitchable3D::default())
                .with_plugin(RenderImgui::<StringBindings>::default()),
        )?;

    let mut game = Application::new(assets_dir, Lightroom::new(0), game_data)?;
    game.run();
    Ok(())
}

#[derive(Default, Debug)]
struct RenderSwitchable3D {
    pbr: RenderPbr3D,
    shaded: RenderShaded3D,
    flat: RenderFlat3D,
    last_mode: RenderMode,
}

impl RenderPlugin<DefaultBackend> for RenderSwitchable3D {
    fn on_build<'a, 'b>(
        &mut self,
        world: &mut World,
        builder: &mut DispatcherBuilder<'a, 'b>,
    ) -> Result<(), Error> {
        <RenderPbr3D as RenderPlugin<DefaultBackend>>::on_build(&mut self.pbr, world, builder)
    }

    fn should_rebuild(&mut self, world: &World) -> bool {
        let mode = *<Read<'_, RenderMode>>::fetch(world);
        self.last_mode != mode
    }

    fn on_plan(
        &mut self,
        plan: &mut RenderPlan<DefaultBackend>,
        factory: &mut Factory<DefaultBackend>,
        world: &World,
    ) -> Result<(), Error> {
        let mode = *<Read<'_, RenderMode>>::fetch(world);
        self.last_mode = mode;
        match mode {
            RenderMode::Pbr => self.pbr.on_plan(plan, factory, world),
            RenderMode::Shaded => self.shaded.on_plan(plan, factory, world),
            RenderMode::Flat => self.flat.on_plan(plan, factory, world),
        }
    }
}

#[derive(Debug, SystemDesc, new)]
#[system_desc(name(SceneChangeSystemDesc))]
pub struct SceneChangeSystem;

impl<'a> System<'a> for SceneChangeSystem {
    type SystemData = (
        Entities<'a>,
        Read<'a, UIState>,
        Write<'a, Scene>,
        Read<'a, SceneMap>,
        WriteStorage<'a, Handle<Prefab<ScenePrefabData>>>,
    );

    fn run(&mut self, (entities, ui_state, mut scene, scene_map, mut prefabs): Self::SystemData) {
        if scene.scene.is_none() || scene.scene.unwrap() != ui_state.scene {
            scene.scene = Some(ui_state.scene);
            let scene_handle = scene_map.get(&ui_state.scene).unwrap().clone();
            if scene.entity.is_some() {
                entities.delete(scene.entity.unwrap());
            }
            println!("Creating new parent entity for scene {}.", scene.scene.unwrap());
            scene.entity = Some(entities.build_entity().with(scene_handle, &mut prefabs).build());
        }
    }
}
#[derive(Debug, SystemDesc, new)]
#[system_desc(name(CustomFreeRotationSystemDesc))]
pub struct CustomFreeRotationSystem {
    sensitivity_x: f32,
    sensitivity_y: f32,
    #[system_desc(event_channel_reader)]
    event_reader: ReaderId<FilteredInputEvent>,
    mouse_down: bool
}

impl<'a> System<'a> for CustomFreeRotationSystem {
    type SystemData = (
        Read<'a, EventChannel<FilteredInputEvent>>,
        WriteStorage<'a, Transform>,
        ReadStorage<'a, FlyControlTag>,
        Read<'a, WindowFocus>,
        Read<'a, HideCursor>,
        Read<'a, UIState>
    );

    fn run(&mut self, (events, mut transform, tag, focus, hide, state): Self::SystemData) {
        #[cfg(feature = "profiler")]
        profile_scope!("free_rotation_system");

        let focused = focus.is_focused;
        for event in events.read(&mut self.event_reader) {
                // if let Event::DeviceEvent { ref event, .. } = *event {
                if let FilteredInputEvent::Free ( ref event ) = *event {
                    match *event {
                    InputEvent::MouseMoved { delta_x, delta_y } => {
                        if focused && hide.hide && (state.free_camera_movement || self.mouse_down) {
                            for (transform, _) in (&mut transform, &tag).join() {
                                transform.append_rotation_x_axis(
                                    (-(delta_y as f32) * self.sensitivity_y).to_radians(),
                                );
                                transform.prepend_rotation_y_axis(
                                    (-(delta_x as f32) * self.sensitivity_x).to_radians(),
                                );
                            }
                        }
                    },
                    InputEvent::MouseButtonPressed ( button ) => if button == MouseButton::Left { self.mouse_down = true },
                    InputEvent::MouseButtonReleased ( button ) => if button == MouseButton::Left { self.mouse_down = false },
                    _ => {},
                }
            }
        }
    }
}

// inspector![
//     Parent,
// 	Transform,
// 	Light,
// ];


#[derive(Default, Debug, Clone)]
pub struct UIState {
    free_camera_movement: bool,
    scene: usize,
}
#[derive(PartialEq, Clone, Debug, Copy)]
pub struct LightTy {
    kind: usize,
    intensity: f32,
    unit_type: usize,
}
#[derive(PartialEq, Clone, Debug, Copy)]
pub struct LightSync {
    entity: Entity,
    translation: [f32; 3],
    rotation: [f32; 4],
    scale: [f32; 2],
    light: LightTy
}

#[derive(Default, Clone, Copy)]
pub struct UISystem;
impl<'s> amethyst::ecs::System<'s> for UISystem {
    type SystemData = (
        Write<'s, UIState>,
        Entities<'s>,
        WriteStorage<'s, Transform>,
        WriteStorage<'s, Light>

    );
    fn run(&mut self, (mut state, entities, mut transforms, mut lights): Self::SystemData) {
        use amethyst_imgui::imgui::*;
        use amethyst::renderer::light::AreaLight;
        let mut lights_cache = (&*entities, &transforms, &lights).join().map(|(e, t, l)| {
            let trans = t.translation();
            let rot = t.rotation().into_inner();

            let scale = t.scale();
            LightSync{
                entity: e,
                translation: [trans[0], trans[1], trans[2]],
                rotation: [rot.coords[0], rot.coords[1], rot.coords[2], rot.coords[3]],
                scale: [scale[0], scale[1]],
                light: if let Light::Area(ref light) = *l {
                        match *light {
                            AreaLight::Disk(ref l) => 
                                LightTy {
                                    kind: 1,
                                    intensity: l.intensity.get(),
                                    unit_type: 0
                                }
                            ,
                            AreaLight::Sphere(ref l) => 
                                LightTy{
                                    kind: 2,
                                    intensity: l.intensity.get(),
                                    unit_type: 0
                                }
                            ,
                            AreaLight::Rectangle(ref l) => 
                                LightTy {
                                    kind: 3,
                                    intensity: l.intensity.get(),
                                    unit_type: 0
                                }
                            ,
                            _ => LightTy {
                                    kind: 0,
                                    intensity: 0.0,
                                    unit_type: 0
                                }
                        }
                    } else {
                        LightTy{
                            kind: 0,
                            intensity: 0.0,
                            unit_type: 0
                        }
                    }
            }
        }).collect::<Vec<_>>();
        let lights_ref = lights_cache.clone();

        amethyst_imgui::with(|ui| {
            Window::new(im_str!("Lighting Example"))
                .size([300.0, 100.0], Condition::FirstUseEver)
                .build(ui, || {
                    ui.checkbox(im_str!("Free camera movement"), &mut state.free_camera_movement);
                    // ui.label_text(im_str!("label"), im_str!("Value"));
                    ComboBox::new(im_str!("Scene")).build_simple_string(ui,
                        &mut state.scene,
                        &[
                            im_str!("Plane"),
                            im_str!("Sponza"),
                        ]);
                    ui.separator();
                    match state.scene {
                        0 => {
                            for mut light in &mut lights_cache {
                                light_ui(ui, &mut light);
                                ui.separator();
                            }
                        },
                        1 => {
                            for mut light in &mut lights_cache {
                                light_ui(ui, &mut light);
                                ui.separator();
                            }
                        }
                        _ => ui.text(im_str!("Please select a scene!")),
                    }
                })
        });
        for (i, light) in lights_cache.iter().enumerate() {
            if light.translation != lights_ref[i].translation {
                transforms.get_mut(light.entity).unwrap().set_translation_xyz(light.translation[0], light.translation[1], light.translation[2]);
            }
            if light.rotation != lights_ref[i].rotation {
                let ijk = Vector3::new(light.rotation[0], light.rotation[1], light.rotation[2]);
                let quarternion = UnitQuaternion::from_quaternion(Quaternion::from_parts(light.rotation[3], ijk));
                transforms.get_mut(light.entity).unwrap().set_rotation(quarternion);
            }
            if light.scale != lights_ref[i].scale {
                let scale = Vector3::new(light.scale[0], light.scale[1], 1.0);
                transforms.get_mut(light.entity).unwrap().set_scale(scale);
            }
            if light.light != lights_ref[i].light {
                let new_light = match light.light.kind {
                    1 => Some(Light::Area(AreaLight::Disk(light::area::Disk {
                        intensity: light::area::Intensity::Power(light.light.intensity),
                        two_sided: true,
                        ..Default::default()
                    }))),
                    2 => Some(Light::Area(AreaLight::Sphere(light::area::Sphere {
                        intensity: light::area::Intensity::Power(light.light.intensity),
                        ..Default::default()
                    }))),
                    3 => Some(Light::Area(AreaLight::Rectangle(light::area::Rectangle {
                        intensity: light::area::Intensity::Power(light.light.intensity),
                        two_sided: true,
                        ..Default::default()
                    }))),
                    _ => None
                };
                if let Some(l) = new_light {
                    lights.insert(light.entity, l);
                }
            }
        }
    }
}

fn show_plane_ui(ui: &amethyst_imgui::imgui::Ui) {
}

fn light_ui(ui: &amethyst_imgui::imgui::Ui, light: &mut LightSync) {
    use amethyst_imgui::imgui::*;
    ui.tree_node(&im_str!("Light: {}", light.entity.id())).build(|| {
        let translation = ui.push_id("translation");
        // Translation
        {
                Slider::new(im_str!("Pos X"), -20.0..=20.0).build(ui, &mut light.translation[0]);
                Slider::new(im_str!("Pos y"), -20.0..=20.0).build(ui, &mut light.translation[1]);
                Slider::new(im_str!("Pos Z"), -20.0..=20.0).build(ui, &mut light.translation[2]);
        }
        translation.pop(ui);
        let rot = ui.push_id("rot");
        // Rotation
        {
            Slider::new(im_str!("X"), -1.0..=1.0).build(ui, &mut light.rotation[0]);
            Slider::new(im_str!("Y"), -1.0..=1.0).build(ui, &mut light.rotation[1]);
            Slider::new(im_str!("Z"), -1.0..=1.0).build(ui, &mut light.rotation[2]);
            Slider::new(im_str!("W"), -1.0..=1.0).build(ui, &mut light.rotation[3]);
        }
        rot.pop(ui);
        let scale = ui.push_id("scale");
        // Rotation
        {
            Slider::new(im_str!("Scale X"), 0.0..=10.0).build(ui, &mut light.scale[0]);
            Slider::new(im_str!("Scale Y"), 0.0..=10.0).build(ui, &mut light.scale[1]);
        }
        scale.pop(ui);
        ui.separator();
        ComboBox::new(im_str!("Scene")).build_simple_string(ui,
            &mut light.light.kind,
            &[
                im_str!("N/A"),
                im_str!("Disk"),
                im_str!("Sphere"),
                im_str!("Rect"),
            ]);
        ComboBox::new(im_str!("Light Unity")).build_simple_string(ui,
            &mut light.light.unit_type,
            &[
                im_str!("Power"),
                im_str!("Luminence"),
            ]);
        if light.light.unit_type == 0 {
            Slider::new(im_str!("Power"), 0.0..=100.0).build(ui, &mut light.light.intensity);
        } else if light.light.unit_type == 1 {
            Slider::new(im_str!("Luminence"), 0.0..=100.0).build(ui, &mut light.light.intensity);
        }
    });
}