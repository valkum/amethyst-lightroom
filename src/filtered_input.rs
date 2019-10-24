use amethyst::{
    core::{
        ecs::{
            Component, DenseVecStorage, DispatcherBuilder, Entities, Entity, Join, Read, ReadExpect,
            ReadStorage, System, SystemData, World, Write, WriteStorage,
        },
        shrev::{EventChannel, ReaderId},
        math::{Unit, UnitQuaternion, Vector3},
        Time, Transform, TransformBundle, SystemDesc
    },
    error::Error,

    input::{
        is_close_requested, is_key_down, is_key_up, Axis, Bindings, Button, InputBundle,
        StringBindings, InputEvent
    },


};
use amethyst::winit::{Event, DeviceEvent, WindowEvent, ElementState, MouseButton};
use amethyst_imgui::ImguiContextWrapper;
use std::sync::{Arc, Mutex};
#[derive(Clone, Debug)]
pub enum FilteredInputEvent {
    Filtered(InputEvent<StringBindings>),
    Free(InputEvent<StringBindings>),
}

pub struct FilterInputSystem {
    input_reader: ReaderId<InputEvent<StringBindings>>,
    winit_reader: ReaderId<Event>,
}
impl<'s> System<'s> for FilterInputSystem {
    type SystemData = (
        ReadExpect<'s, Arc<Mutex<ImguiContextWrapper>>>,
        Read<'s, EventChannel<InputEvent<StringBindings>>>,
        Read<'s, EventChannel<Event>>,
        Write<'s, EventChannel<FilteredInputEvent>>,
    );

    fn run(
        &mut self,
        (context, input_events, winit_events, mut filtered_events): Self::SystemData,
    ) {
        let state = &mut context.lock().unwrap().0;

        for _ in winit_events.read(&mut self.winit_reader) {}
        for input in input_events.read(&mut self.input_reader) {
            let mut taken = false;
            match input {
                InputEvent::MouseMoved { .. }
                | InputEvent::MouseButtonPressed(_)
                | InputEvent::MouseButtonReleased(_)
                | InputEvent::MouseWheelMoved(_) => {
                    if state.io().want_capture_mouse {
                        taken = true;
                    }
                }
                InputEvent::KeyPressed { .. } | InputEvent::KeyReleased { .. } => {
                    if state.io().want_capture_keyboard {
                        taken = true;
                    }
                }
                InputEvent::ActionPressed(action) => match action {
                    _ => {
                        if state.io().want_capture_mouse {
                            taken = true;
                        }
                    }
                },
                InputEvent::ActionReleased(action) => match action {
                    _ => {
                        if state.io().want_capture_mouse || state.io().want_capture_keyboard {
                            taken = true;
                        }
                    }
                },
                _ => {}
            }

            if taken {
                filtered_events.single_write(FilteredInputEvent::Filtered(input.clone()));
            } else {
                filtered_events.single_write(FilteredInputEvent::Free(input.clone()));
            }
        }
    }
}

#[derive(Default)]
pub struct FilterInputSystemDesc;
impl<'a, 'b> SystemDesc<'a, 'b, FilterInputSystem> for FilterInputSystemDesc {
    fn build(self, world: &mut World) -> FilterInputSystem {
        <FilterInputSystem as System<'_>>::SystemData::setup(world);

        let input_reader =
            Write::<EventChannel<InputEvent<StringBindings>>>::fetch(world).register_reader();
        let winit_reader = Write::<EventChannel<Event>>::fetch(world).register_reader();

        FilterInputSystem {
            input_reader,
            winit_reader,
        }
    }
}