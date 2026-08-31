use bevy::prelude::*;
use game_utils_bevy::loading::{LoadingProgress, LoadingTip, assets_progress};
use game_utils_bevy::transitions::Transition;

use crate::app::AppState;
use crate::asset_tracking::AssetsLoading;

pub struct ScreensPlugin;
impl Plugin for ScreensPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<LoadingProgress>()
            .init_resource::<LoadingTip>()
            .add_systems(
                OnEnter(AppState::Loading),
                (
                    |mut c: Commands,
                     asset_server: Res<AssetServer>,
                     mut progress: ResMut<LoadingProgress>,
                     mut tip: ResMut<LoadingTip>| {
                        c.insert_resource(LoadingTimer(Timer::from_seconds(
                            0.5,
                            TimerMode::Once,
                        )));
                        let handles =
                            vec![asset_server.load::<Font>("fonts/default.ttf").untyped()];
                        c.insert_resource(AssetsLoading(handles));
                        progress.0 = 0.0;
                        // Modular tip pool (game-utils LoadingTip) - mimics GenCont tip
                        tip.0 = "GENERATING... 0%".to_string();
                    },
                )
                    .chain(),
            )
            .add_systems(Update, tick_loading)
            .add_systems(OnExit(AppState::Loading), |mut c: Commands| {
                c.remove_resource::<LoadingTimer>();
                c.remove_resource::<AssetsLoading>();
            });
    }
}

#[derive(Resource)]
struct LoadingTimer(Timer);

fn tick_loading(
    time: Res<Time<Real>>,
    mut tr: ResMut<Transition<AppState>>,
    asset_server: Res<AssetServer>,
    timer: Option<ResMut<LoadingTimer>>,
    assets: Option<Res<AssetsLoading>>,
    mut progress: ResMut<LoadingProgress>,
    mut tip: ResMut<LoadingTip>,
) {
    let Some(mut timer) = timer else { return };
    // Modular progress via game-utils helper
    let prog = assets
        .as_ref()
        .map(|a| assets_progress(&a.0, &asset_server))
        .unwrap_or(1.0);
    progress.0 = prog;
    // Update tip like GenCont: "GENERATING... XX%"
    tip.0 = format!("GENERATING... {}%", (prog * 100.0).round() as u32);

    if prog >= 1.0 && timer.0.tick(time.delta()).just_finished() {
        // Loading -> InGame: simple fade, spiral stays via Loading state
        tr.begin_to_state(AppState::InGame);
    }
}
