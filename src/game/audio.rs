//! SFX: placeholder WAVs in `assets/audio/` (generated) or original
//! `.ogg` (Vorbis) imported locally via `tools/gen_assets.py`.  No copyrighted
//! assets are committed; `.ogg` are loaded directly (Bevy `vorbis` feature)
//! without conversion. `GameAudio` holds handles loaded once at startup.

use bevy::prelude::*;
use game_utils_bevy::audio::AudioM;

#[derive(Resource)]
pub struct GameAudio {
    pub shoot: Handle<AudioSource>,
    pub machine: Handle<AudioSource>,
    pub shotgun: Handle<AudioSource>,
    pub bolt: Handle<AudioSource>,
    pub melee: Handle<AudioSource>,
    pub explode: Handle<AudioSource>,
    pub boom: Handle<AudioSource>,
    pub hit: Handle<AudioSource>,
    pub hurt: Handle<AudioSource>,
    pub pickup: Handle<AudioSource>,
    pub levelup: Handle<AudioSource>,
    pub portal: Handle<AudioSource>,
    pub death: Handle<AudioSource>,
    pub chest: Handle<AudioSource>,
}

fn load_sfx(asset_server: &AssetServer, stem: &str) -> Handle<AudioSource> {
    let _wav: Handle<AudioSource> = asset_server.load(format!("audio/{stem}.wav"));
    asset_server.load(format!("audio/{stem}.ogg"))
}

impl GameAudio {
    pub fn load(asset_server: &AssetServer) -> Self {
        Self {
            shoot: load_sfx(asset_server, "sndPistol"),
            machine: load_sfx(asset_server, "sndMachinegun"),
            shotgun: load_sfx(asset_server, "sndShotgun"),
            bolt: load_sfx(asset_server, "sndCrossbow"),
            melee: load_sfx(asset_server, "sndHammer"),
            explode: load_sfx(asset_server, "sndExplosion"),
            boom: load_sfx(asset_server, "sndExplosionL"),
            hit: load_sfx(asset_server, "sndHitWall"),
            hurt: load_sfx(asset_server, "snd_hurt"),
            pickup: load_sfx(asset_server, "sndAmmoPickup"),
            levelup: load_sfx(asset_server, "sndLevelUp"),
            portal: load_sfx(asset_server, "sndPortalOpen"),
            death: load_sfx(asset_server, "snd_dead"),
            chest: load_sfx(asset_server, "sndChest"),
        }
    }

    pub fn play_shoot(&self, commands: &mut Commands) {
        AudioM::play_sfx_varied(commands, self.shoot.clone(), 0.5, 0.12);
    }

    pub fn play_machine(&self, commands: &mut Commands) {
        AudioM::play_sfx_varied(commands, self.machine.clone(), 0.4, 0.15);
    }

    pub fn play_shotgun(&self, commands: &mut Commands) {
        AudioM::play_sfx_varied(commands, self.shotgun.clone(), 0.6, 0.1);
    }

    pub fn play_bolt(&self, commands: &mut Commands) {
        AudioM::play_sfx_varied(commands, self.bolt.clone(), 0.5, 0.08);
    }

    pub fn play_melee(&self, commands: &mut Commands) {
        AudioM::play_sfx_varied(commands, self.melee.clone(), 0.5, 0.1);
    }

    pub fn play_explode(&self, commands: &mut Commands) {
        AudioM::play_sfx_varied(commands, self.explode.clone(), 0.7, 0.06);
    }

    pub fn play_boom(&self, commands: &mut Commands) {
        AudioM::play_sfx_varied(commands, self.boom.clone(), 0.9, 0.04);
    }

    pub fn play_hit(&self, commands: &mut Commands) {
        AudioM::play_sfx_varied(commands, self.hit.clone(), 0.45, 0.15);
    }

    pub fn play_hurt(&self, commands: &mut Commands) {
        AudioM::play_sfx_varied(commands, self.hurt.clone(), 0.7, 0.05);
    }

    pub fn play_pickup(&self, commands: &mut Commands) {
        AudioM::play_sfx_varied(commands, self.pickup.clone(), 0.5, 0.15);
    }

    pub fn play_levelup(&self, commands: &mut Commands) {
        AudioM::play_sfx_varied(commands, self.levelup.clone(), 0.8, 0.03);
    }

    pub fn play_portal(&self, commands: &mut Commands) {
        AudioM::play_sfx_varied(commands, self.portal.clone(), 0.7, 0.05);
    }

    pub fn play_death(&self, commands: &mut Commands) {
        AudioM::play_sfx_varied(commands, self.death.clone(), 0.9, 0.02);
    }

    pub fn play_chest(&self, commands: &mut Commands) {
        AudioM::play_sfx_varied(commands, self.chest.clone(), 0.6, 0.05);
    }
}
