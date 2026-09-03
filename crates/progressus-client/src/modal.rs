use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use progressus_app::{
    ClientSnapshot, Command, EntityId, MAX_PRODUCTION_ORDER_RUNS, ProductionOrderSnapshot,
    ProductionTarget, ProductionZoneKind, RecipeId, WorkstationKind,
};

use crate::i18n::{Locale, TextKey};
use crate::procedural_assets::{ProceduralAssetParams, workstation_asset};
use crate::runtime::AuthoritativeClient;
use crate::ui::{ToolMode, ToolState, UiCapture};
use crate::ui_font::UiFont;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ModalKind {
    Workstation(EntityId),
}

#[derive(Resource, Debug, Default)]
pub(crate) struct ModalState {
    pub(crate) open: Option<ModalKind>,
    dirty: bool,
}

impl ModalState {
    pub(crate) fn open_workstation(&mut self, workstation_id: EntityId) {
        self.open = Some(ModalKind::Workstation(workstation_id));
        self.dirty = true;
    }

    pub(crate) fn close(&mut self) {
        self.open = None;
        self.dirty = true;
    }

    pub(crate) const fn is_open(&self) -> bool {
        self.open.is_some()
    }
}

#[derive(Resource, Default)]
pub(crate) struct ModalPresentation {
    root: Option<Entity>,
    signature: Option<(ModalKind, u64, u64, u64, crate::i18n::Language)>,
}

#[derive(Component)]
pub(crate) struct ModalCloseButton;

#[derive(Component)]
pub(crate) struct AddOrderButton {
    workstation_id: EntityId,
    recipe_id: RecipeId,
}

#[derive(Component)]
pub(crate) struct AddInfiniteOrderButton {
    workstation_id: EntityId,
    recipe_id: RecipeId,
}

#[derive(Component)]
pub(crate) struct AdjustOrderButton {
    order_id: EntityId,
    delta: i32,
}

#[derive(Component)]
pub(crate) struct DeleteOrderButton {
    order_id: EntityId,
}

#[derive(Component)]
pub(crate) struct ToggleInfiniteButton {
    order_id: EntityId,
}

#[derive(Component)]
pub(crate) struct RemoveWorkstationButton {
    workstation_id: EntityId,
}

#[derive(Component)]
pub(crate) struct RotateWorkbenchInputsButton {
    workstation_id: EntityId,
}

#[derive(Component)]
pub(crate) struct RotateWorkbenchOutputsButton {
    workstation_id: EntityId,
}

#[derive(Component)]
pub(crate) struct EditProductionZoneButton {
    workstation_id: EntityId,
    kind: ProductionZoneKind,
    enabled: bool,
}

#[derive(SystemParam)]
pub(crate) struct ModalInteractionQueries<'w, 's> {
    close: Query<'w, 's, &'static Interaction, (Changed<Interaction>, With<ModalCloseButton>)>,
    add: Query<'w, 's, (&'static Interaction, &'static AddOrderButton), Changed<Interaction>>,
    add_infinite: Query<
        'w,
        's,
        (&'static Interaction, &'static AddInfiniteOrderButton),
        Changed<Interaction>,
    >,
    adjust: Query<'w, 's, (&'static Interaction, &'static AdjustOrderButton), Changed<Interaction>>,
    toggle_infinite:
        Query<'w, 's, (&'static Interaction, &'static ToggleInfiniteButton), Changed<Interaction>>,
    delete: Query<'w, 's, (&'static Interaction, &'static DeleteOrderButton), Changed<Interaction>>,
    remove: Query<
        'w,
        's,
        (&'static Interaction, &'static RemoveWorkstationButton),
        Changed<Interaction>,
    >,
    rotate_inputs: Query<
        'w,
        's,
        (&'static Interaction, &'static RotateWorkbenchInputsButton),
        Changed<Interaction>,
    >,
    rotate_outputs: Query<
        'w,
        's,
        (&'static Interaction, &'static RotateWorkbenchOutputsButton),
        Changed<Interaction>,
    >,
    edit_zone: Query<
        'w,
        's,
        (&'static Interaction, &'static EditProductionZoneButton),
        Changed<Interaction>,
    >,
}

const PANEL: Color = Color::srgba(0.055, 0.065, 0.07, 0.985);
const ROW: Color = Color::srgba(0.10, 0.12, 0.13, 0.98);
const BUTTON: Color = Color::srgb(0.16, 0.19, 0.20);
const DANGER: Color = Color::srgb(0.40, 0.13, 0.12);
const TEXT: Color = Color::srgb(0.93, 0.95, 0.96);
const MUTED: Color = Color::srgb(0.70, 0.75, 0.77);

pub(crate) fn modal_keyboard(keys: Res<ButtonInput<KeyCode>>, mut state: ResMut<ModalState>) {
    if state.is_open() && keys.just_pressed(KeyCode::Escape) {
        state.close();
    }
}

pub(crate) fn sync_modal(
    mut commands: Commands,
    authoritative: Res<AuthoritativeClient>,
    mut state: ResMut<ModalState>,
    mut presentation: ResMut<ModalPresentation>,
    locale: Res<Locale>,
    font: Res<UiFont>,
    mut procedural: ProceduralAssetParams,
) {
    let Some(kind) = state.open else {
        if let Some(root) = presentation.root.take() {
            commands.entity(root).despawn();
        }
        presentation.signature = None;
        state.dirty = false;
        return;
    };

    let signature = (
        kind,
        authoritative.snapshot().production_revision,
        authoritative.snapshot().workstation_revision,
        authoritative.snapshot().production_logistics_revision,
        locale.language,
    );
    if !state.dirty && presentation.signature == Some(signature) {
        return;
    }
    if let Some(root) = presentation.root.take() {
        commands.entity(root).despawn();
    }

    let root = match kind {
        ModalKind::Workstation(workstation_id) => {
            let Some(workstation) = authoritative
                .snapshot()
                .workstations
                .iter()
                .find(|workstation| workstation.id == workstation_id)
                .copied()
            else {
                state.close();
                presentation.signature = None;
                return;
            };
            let workbench_image = {
                let (images, registry) = procedural.parts();
                registry.image_handle(images, workstation_asset(workstation.kind, workstation_id))
            };
            spawn_workstation_modal(
                &mut commands,
                authoritative.snapshot(),
                workstation_id,
                workstation.kind,
                *locale,
                &font,
                workbench_image,
            )
        }
    };

    presentation.root = Some(root);
    presentation.signature = Some(signature);
    state.dirty = false;
}

fn text_bundle(text: impl Into<String>, font: &UiFont, size: f32, color: Color) -> impl Bundle {
    (
        Text::new(text),
        TextFont {
            font: font.0.clone(),
            font_size: size,
            ..default()
        },
        TextColor(color),
    )
}

fn button_node() -> Node {
    Node {
        height: px(32),
        padding: UiRect::horizontal(px(9)),
        justify_content: JustifyContent::Center,
        align_items: AlignItems::Center,
        border: UiRect::all(px(1)),
        ..default()
    }
}

fn spawn_workstation_modal(
    commands: &mut Commands,
    snapshot: &ClientSnapshot,
    workstation_id: EntityId,
    workstation_kind: WorkstationKind,
    locale: Locale,
    font: &UiFont,
    workbench_image: Handle<Image>,
) -> Entity {
    let orders = snapshot
        .production_orders
        .iter()
        .filter(|order| order.workstation_id == workstation_id)
        .copied()
        .collect::<Vec<_>>();

    commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                width: percent(100),
                height: percent(100),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                ..default()
            },
            BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.55)),
            GlobalZIndex(100),
            Interaction::default(),
            UiCapture,
        ))
        .with_children(|backdrop| {
            backdrop
                .spawn((
                    Node {
                        width: px(600),
                        min_height: px(410),
                        padding: UiRect::all(px(16)),
                        row_gap: px(12),
                        flex_direction: FlexDirection::Column,
                        border: UiRect::all(px(1)),
                        ..default()
                    },
                    BackgroundColor(PANEL),
                    BorderColor::all(Color::srgb(0.30, 0.36, 0.38)),
                    UiCapture,
                ))
                .with_children(|panel| {
                    spawn_title_row(panel, workstation_id, workstation_kind, locale, font);
                    spawn_logistics(
                        panel,
                        snapshot,
                        workstation_id,
                        locale,
                        font,
                        &workbench_image,
                    );
                    spawn_recipe_row(panel, workstation_id, locale, font);
                    spawn_orders(panel, &orders, locale, font);
                    spawn_footer(panel, workstation_id, locale, font);
                });
        })
        .id()
}

fn spawn_title_row(
    panel: &mut ChildSpawnerCommands,
    workstation_id: EntityId,
    workstation_kind: WorkstationKind,
    locale: Locale,
    font: &UiFont,
) {
    panel
        .spawn(Node {
            width: percent(100),
            justify_content: JustifyContent::SpaceBetween,
            align_items: AlignItems::Center,
            ..default()
        })
        .with_children(|row| {
            row.spawn(text_bundle(
                format!(
                    "{}  #{}",
                    locale.workstation_name(workstation_kind),
                    workstation_id.value()
                ),
                font,
                22.0,
                TEXT,
            ));
            row.spawn((
                Button,
                ModalCloseButton,
                UiCapture,
                button_node(),
                BackgroundColor(BUTTON),
            ))
            .with_children(|button| {
                button.spawn(text_bundle(locale.tr(TextKey::Close), font, 14.0, TEXT));
            });
        });
}

fn spawn_logistics(
    panel: &mut ChildSpawnerCommands,
    snapshot: &ClientSnapshot,
    workstation_id: EntityId,
    locale: Locale,
    font: &UiFont,
    workbench_image: &Handle<Image>,
) {
    let workstation = snapshot
        .workstations
        .iter()
        .find(|item| item.id == workstation_id);
    let logistics = snapshot
        .production_logistics
        .iter()
        .find(|logistics| logistics.workstation_id == workstation_id);
    panel.spawn(text_bundle(
        locale.tr(TextKey::Logistics),
        font,
        16.0,
        MUTED,
    ));
    panel
        .spawn((
            Node {
                width: percent(100),
                padding: UiRect::all(px(10)),
                column_gap: px(14),
                align_items: AlignItems::Center,
                ..default()
            },
            BackgroundColor(ROW),
        ))
        .with_children(|row| {
            row.spawn(Node {
                width: px(112),
                height: px(112),
                position_type: PositionType::Relative,
                ..default()
            })
            .with_children(|diagram| {
                diagram.spawn((
                    ImageNode::new(workbench_image.clone()),
                    Node {
                        position_type: PositionType::Absolute,
                        left: px(32),
                        top: px(32),
                        width: px(48),
                        height: px(48),
                        ..default()
                    },
                ));
                if let (Some(workstation), Some(logistics)) = (workstation, logistics) {
                    for cell in &logistics.input_cells {
                        let dx = cell.x() - workstation.cell.x();
                        let dy = cell.y() - workstation.cell.y();
                        let (left, top) = match (dx, dy) {
                            (0, 1) => (50.0, 8.0),
                            (1, 0) => (92.0, 50.0),
                            (0, -1) => (50.0, 92.0),
                            (-1, 0) => (8.0, 50.0),
                            _ => continue,
                        };
                        diagram.spawn((
                            Node {
                                position_type: PositionType::Absolute,
                                left: px(left),
                                top: px(top),
                                width: px(12),
                                height: px(12),
                                border_radius: BorderRadius::MAX,
                                ..default()
                            },
                            BackgroundColor(Color::srgb(0.88, 0.16, 0.14)),
                        ));
                    }
                    for cell in &logistics.output_cells {
                        let dx = cell.x() - workstation.cell.x();
                        let dy = cell.y() - workstation.cell.y();
                        let (left, top) = match (dx, dy) {
                            (-1, 1) => (18.0, 18.0),
                            (1, 1) => (84.0, 18.0),
                            (1, -1) => (84.0, 84.0),
                            (-1, -1) => (18.0, 84.0),
                            _ => continue,
                        };
                        diagram.spawn((
                            Node {
                                position_type: PositionType::Absolute,
                                left: px(left),
                                top: px(top),
                                width: px(12),
                                height: px(12),
                                border_radius: BorderRadius::MAX,
                                ..default()
                            },
                            BackgroundColor(Color::srgb(0.98, 0.84, 0.12)),
                        ));
                    }
                }
            });
            row.spawn(Node {
                flex_direction: FlexDirection::Column,
                row_gap: px(8),
                ..default()
            })
            .with_children(|actions| {
                actions.spawn(text_bundle(
                    locale.tr(TextKey::InputPorts),
                    font,
                    15.0,
                    TEXT,
                ));
                actions
                    .spawn((
                        Button,
                        RotateWorkbenchInputsButton { workstation_id },
                        UiCapture,
                        button_node(),
                        BackgroundColor(BUTTON),
                    ))
                    .with_children(|button| {
                        button.spawn(text_bundle(
                            locale.tr(TextKey::RotateInputs),
                            font,
                            13.0,
                            TEXT,
                        ));
                    });
                actions.spawn(text_bundle(
                    locale.tr(TextKey::OutputPorts),
                    font,
                    15.0,
                    TEXT,
                ));
                actions
                    .spawn((
                        Button,
                        RotateWorkbenchOutputsButton { workstation_id },
                        UiCapture,
                        button_node(),
                        BackgroundColor(BUTTON),
                    ))
                    .with_children(|button| {
                        button.spawn(text_bundle(
                            locale.tr(TextKey::RotateOutputs),
                            font,
                            13.0,
                            TEXT,
                        ));
                    });
            });
        });
}

fn spawn_recipe_row(
    panel: &mut ChildSpawnerCommands,
    workstation_id: EntityId,
    locale: Locale,
    font: &UiFont,
) {
    panel.spawn(text_bundle(locale.tr(TextKey::Recipes), font, 16.0, MUTED));
    panel
        .spawn((
            Node {
                width: percent(100),
                padding: UiRect::all(px(10)),
                justify_content: JustifyContent::SpaceBetween,
                align_items: AlignItems::Center,
                ..default()
            },
            BackgroundColor(ROW),
        ))
        .with_children(|row| {
            row.spawn(text_bundle(
                locale.recipe_name(RecipeId::PrimitiveTool),
                font,
                16.0,
                TEXT,
            ));
            row.spawn(Node {
                column_gap: px(7),
                align_items: AlignItems::Center,
                ..default()
            })
            .with_children(|actions| {
                actions
                    .spawn((
                        Button,
                        AddOrderButton {
                            workstation_id,
                            recipe_id: RecipeId::PrimitiveTool,
                        },
                        UiCapture,
                        button_node(),
                        BackgroundColor(BUTTON),
                    ))
                    .with_children(|button| {
                        button.spawn(text_bundle(locale.tr(TextKey::AddOrder), font, 14.0, TEXT));
                    });
                actions
                    .spawn((
                        Button,
                        AddInfiniteOrderButton {
                            workstation_id,
                            recipe_id: RecipeId::PrimitiveTool,
                        },
                        UiCapture,
                        button_node(),
                        BackgroundColor(BUTTON),
                    ))
                    .with_children(|button| {
                        button.spawn(text_bundle("∞", font, 18.0, TEXT));
                    });
            });
        });
}

fn spawn_orders(
    panel: &mut ChildSpawnerCommands,
    orders: &[ProductionOrderSnapshot],
    locale: Locale,
    font: &UiFont,
) {
    panel.spawn(text_bundle(locale.tr(TextKey::Orders), font, 16.0, MUTED));
    if orders.is_empty() {
        panel.spawn(text_bundle(locale.tr(TextKey::NoOrders), font, 14.0, MUTED));
        return;
    }

    for order in orders {
        panel
            .spawn((
                Node {
                    width: percent(100),
                    padding: UiRect::all(px(10)),
                    column_gap: px(7),
                    align_items: AlignItems::Center,
                    ..default()
                },
                BackgroundColor(ROW),
            ))
            .with_children(|row| {
                row.spawn((
                    Node {
                        flex_grow: 1.0,
                        ..default()
                    },
                    text_bundle(locale.recipe_name(order.recipe_id), font, 15.0, TEXT),
                ));
                let target_label = match order.target {
                    ProductionTarget::Finite { remaining_runs } => remaining_runs.to_string(),
                    ProductionTarget::Infinite => "∞".to_owned(),
                };
                row.spawn(text_bundle(
                    format!("{}: {}", locale.tr(TextKey::Remaining), target_label),
                    font,
                    14.0,
                    TEXT,
                ));
                if matches!(order.target, ProductionTarget::Finite { .. }) {
                    for (label, delta) in [("-10", -10), ("-1", -1), ("+1", 1), ("+10", 10)] {
                        row.spawn((
                            Button,
                            AdjustOrderButton {
                                order_id: order.id,
                                delta,
                            },
                            UiCapture,
                            button_node(),
                            BackgroundColor(BUTTON),
                        ))
                        .with_children(|button| {
                            button.spawn(text_bundle(label, font, 13.0, TEXT));
                        });
                    }
                }
                row.spawn((
                    Button,
                    ToggleInfiniteButton { order_id: order.id },
                    UiCapture,
                    button_node(),
                    BackgroundColor(if matches!(order.target, ProductionTarget::Infinite) {
                        Color::srgb(0.10, 0.46, 0.58)
                    } else {
                        BUTTON
                    }),
                ))
                .with_children(|button| {
                    button.spawn(text_bundle("∞", font, 16.0, TEXT));
                });
                row.spawn((
                    Button,
                    DeleteOrderButton { order_id: order.id },
                    UiCapture,
                    button_node(),
                    BackgroundColor(DANGER),
                ))
                .with_children(|button| {
                    button.spawn(text_bundle(locale.tr(TextKey::Delete), font, 13.0, TEXT));
                });
            });
    }
}

fn spawn_footer(
    panel: &mut ChildSpawnerCommands,
    workstation_id: EntityId,
    locale: Locale,
    font: &UiFont,
) {
    panel
        .spawn(Node {
            width: percent(100),
            margin: UiRect::top(px(6)),
            justify_content: JustifyContent::FlexEnd,
            column_gap: px(8),
            ..default()
        })
        .with_children(|row| {
            row.spawn((
                Button,
                RemoveWorkstationButton { workstation_id },
                UiCapture,
                button_node(),
                BackgroundColor(DANGER),
            ))
            .with_children(|button| {
                button.spawn(text_bundle(
                    locale.tr(TextKey::RemoveWorkstation),
                    font,
                    14.0,
                    TEXT,
                ));
            });
            row.spawn((
                Button,
                ModalCloseButton,
                UiCapture,
                button_node(),
                BackgroundColor(BUTTON),
            ))
            .with_children(|button| {
                button.spawn(text_bundle(locale.tr(TextKey::Close), font, 14.0, TEXT));
            });
        });
}

pub(crate) fn modal_interaction(
    mut authoritative: ResMut<AuthoritativeClient>,
    mut state: ResMut<ModalState>,
    mut tool: ResMut<ToolState>,
    interactions: ModalInteractionQueries,
) {
    let ModalInteractionQueries {
        close,
        add,
        add_infinite,
        adjust,
        toggle_infinite,
        delete,
        remove,
        rotate_inputs,
        rotate_outputs,
        edit_zone,
    } = interactions;
    if close
        .iter()
        .any(|interaction| *interaction == Interaction::Pressed)
    {
        state.close();
        return;
    }

    for (interaction, button) in &add {
        if *interaction != Interaction::Pressed {
            continue;
        }
        if let Err(error) = authoritative
            .application_mut()
            .execute(Command::AddProductionOrder {
                workstation_id: button.workstation_id,
                recipe_id: button.recipe_id,
                target: ProductionTarget::finite(1),
            })
        {
            warn!("production order rejected: {error}");
        } else {
            refresh(&mut authoritative);
            state.dirty = true;
        }
    }

    for (interaction, button) in &add_infinite {
        if *interaction != Interaction::Pressed {
            continue;
        }
        if let Err(error) = authoritative
            .application_mut()
            .execute(Command::AddProductionOrder {
                workstation_id: button.workstation_id,
                recipe_id: button.recipe_id,
                target: ProductionTarget::Infinite,
            })
        {
            warn!("infinite production order rejected: {error}");
        } else {
            refresh(&mut authoritative);
            state.dirty = true;
        }
    }

    for (interaction, button) in &adjust {
        if *interaction != Interaction::Pressed {
            continue;
        }
        let Some(order) = authoritative
            .snapshot()
            .production_orders
            .iter()
            .find(|order| order.id == button.order_id)
            .copied()
        else {
            continue;
        };
        let ProductionTarget::Finite { remaining_runs } = order.target else {
            continue;
        };
        let next = if button.delta.is_negative() {
            remaining_runs.saturating_sub(button.delta.unsigned_abs())
        } else {
            remaining_runs
                .saturating_add(button.delta as u32)
                .min(MAX_PRODUCTION_ORDER_RUNS)
        };
        if let Err(error) =
            authoritative
                .application_mut()
                .execute(Command::SetProductionOrderTarget {
                    order_id: button.order_id,
                    target: ProductionTarget::finite(next),
                })
        {
            warn!("production order update rejected: {error}");
        } else {
            refresh(&mut authoritative);
            state.dirty = true;
        }
    }

    for (interaction, button) in &toggle_infinite {
        if *interaction != Interaction::Pressed {
            continue;
        }
        let Some(order) = authoritative
            .snapshot()
            .production_orders
            .iter()
            .find(|order| order.id == button.order_id)
            .copied()
        else {
            continue;
        };
        let target = match order.target {
            ProductionTarget::Infinite => ProductionTarget::finite(1),
            ProductionTarget::Finite { .. } => ProductionTarget::Infinite,
        };
        if let Err(error) =
            authoritative
                .application_mut()
                .execute(Command::SetProductionOrderTarget {
                    order_id: button.order_id,
                    target,
                })
        {
            warn!("production order mode update rejected: {error}");
        } else {
            refresh(&mut authoritative);
            state.dirty = true;
        }
    }

    for (interaction, button) in &delete {
        if *interaction != Interaction::Pressed {
            continue;
        }
        if let Err(error) =
            authoritative
                .application_mut()
                .execute(Command::RemoveProductionOrder {
                    order_id: button.order_id,
                })
        {
            warn!("production order removal rejected: {error}");
        } else {
            refresh(&mut authoritative);
            state.dirty = true;
        }
    }

    for (interaction, button) in &rotate_inputs {
        if *interaction != Interaction::Pressed {
            continue;
        }
        if let Err(error) =
            authoritative
                .application_mut()
                .execute(Command::CycleWorkstationInputs {
                    workstation_id: button.workstation_id,
                })
        {
            warn!("workstation input rotation rejected: {error}");
        } else {
            refresh(&mut authoritative);
            state.dirty = true;
        }
    }

    for (interaction, button) in &rotate_outputs {
        if *interaction != Interaction::Pressed {
            continue;
        }
        if let Err(error) =
            authoritative
                .application_mut()
                .execute(Command::CycleWorkstationOutputs {
                    workstation_id: button.workstation_id,
                })
        {
            warn!("workstation output rotation rejected: {error}");
        } else {
            refresh(&mut authoritative);
            state.dirty = true;
        }
    }

    for (interaction, button) in &edit_zone {
        if *interaction != Interaction::Pressed {
            continue;
        }
        tool.mode = if button.enabled {
            ToolMode::ProductionZoneAdd {
                workstation_id: button.workstation_id,
                kind: button.kind,
            }
        } else {
            ToolMode::ProductionZoneRemove {
                workstation_id: button.workstation_id,
                kind: button.kind,
            }
        };
        tool.cancel_drag();
        state.close();
        return;
    }

    for (interaction, button) in &remove {
        if *interaction != Interaction::Pressed {
            continue;
        }
        if let Err(error) = authoritative
            .application_mut()
            .execute(Command::RemoveWorkstation {
                workstation_id: button.workstation_id,
            })
        {
            warn!("workstation removal rejected: {error}");
        } else {
            refresh(&mut authoritative);
            state.close();
        }
    }
}

fn refresh(authoritative: &mut AuthoritativeClient) {
    if let Err(error) = authoritative.refresh_lightweight_snapshot(None) {
        error!("authoritative snapshot failed after modal action: {error}");
    }
}
