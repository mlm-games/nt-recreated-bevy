//! SFX: placeholder WAVs in `assets/audio/` (generated) or original
//! `.ogg` (Vorbis) imported locally via `tools/gen_assets.py`.  No copyrighted
//! assets are committed; `.ogg` are loaded directly (Bevy `vorbis` feature)
//! without conversion. `GameAudio` holds handles loaded once at startup.

use bevy::prelude::*;
use game_utils_bevy::audio::AudioM;

use crate::game::content::AssetCatalog;

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
    /// Upstream sndWeaponChest / sndAmmoChest (per chest kind).
    pub weapon_chest: Handle<AudioSource>,
    pub ammo_chest: Handle<AudioSource>,
    /// sndPickupDisappear - rad/HP/ammo blink-out.
    pub pickup_disappear: Handle<AudioSource>,
    /// GML sndEmpty / sndUltraEmpty (scrEmpty / scrEmptyRads).
    pub empty: Handle<AudioSource>,
    pub ultra_empty: Handle<AudioSource>,
    /// Per-family fire sounds (GML scrFire snd table).
    pub plasma: Handle<AudioSource>,
    pub laser: Handle<AudioSource>,
    pub lightning: Handle<AudioSource>,
    pub flame: Handle<AudioSource>,
    pub disc: Handle<AudioSource>,
    pub slugger: Handle<AudioSource>,
    pub grenade: Handle<AudioSource>,
    pub splinter: Handle<AudioSource>,
}

fn resolve_sfx(catalog: &AssetCatalog, stem: &str) -> String {
    // Original imported files are OGG. Generated placeholders are WAV.
    // Prefer originals whenever present.
    for dir in ["audio", "sounds"] {
        for ext in ["ogg", "wav", "mp3", "flac"] {
            let path = format!("{dir}/{stem}.{ext}");
            if catalog.has_audio(&path) {
                return path;
            }
        }
    }

    // Keep the old fallback so dev builds with generated placeholders still run.
    format!("audio/{stem}.wav")
}

fn load_sfx(asset_server: &AssetServer, catalog: &AssetCatalog, stem: &str) -> Handle<AudioSource> {
    asset_server.load(resolve_sfx(catalog, stem))
}
impl GameAudio {
    pub fn load(asset_server: &AssetServer, catalog: &AssetCatalog) -> Self {
        Self {
            shoot: load_sfx(asset_server, catalog, "sndPistol"),
            machine: load_sfx(asset_server, catalog, "sndMachinegun"),
            shotgun: load_sfx(asset_server, catalog, "sndShotgun"),
            bolt: load_sfx(asset_server, catalog, "sndCrossbow"),
            melee: load_sfx(asset_server, catalog, "sndHammer"),
            explode: load_sfx(asset_server, catalog, "sndExplosion"),
            boom: load_sfx(asset_server, catalog, "sndExplosionL"),
            hit: load_sfx(asset_server, catalog, "sndHitWall"),
            hurt: load_sfx(asset_server, catalog, "sndPlayerHit"),
            pickup: load_sfx(asset_server, catalog, "sndAmmoPickup"),
            levelup: load_sfx(asset_server, catalog, "sndLevelUp"),
            portal: load_sfx(asset_server, catalog, "sndPortalOpen"),
            death: load_sfx(asset_server, catalog, "sndPlayerDeath"),
            chest: load_sfx(asset_server, catalog, "sndChest"),
            weapon_chest: load_sfx(asset_server, catalog, "sndWeaponChest"),
            ammo_chest: load_sfx(asset_server, catalog, "sndAmmoChest"),
            pickup_disappear: load_sfx(asset_server, catalog, "sndPickupDisappear"),
            empty: load_sfx(asset_server, catalog, "sndEmpty"),
            ultra_empty: load_sfx(asset_server, catalog, "sndUltraEmpty"),
            plasma: load_sfx(asset_server, catalog, "sndPlasma"),
            laser: load_sfx(asset_server, catalog, "sndLaser"),
            lightning: load_sfx(asset_server, catalog, "sndLightningPistol"),
            flame: load_sfx(asset_server, catalog, "sndFlameCannon"),
            disc: load_sfx(asset_server, catalog, "sndDiscgun"),
            slugger: load_sfx(asset_server, catalog, "sndSlugger"),
            grenade: load_sfx(asset_server, catalog, "sndGrenade"),
            splinter: load_sfx(asset_server, catalog, "sndSplinterGun"),
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

    pub fn play_weapon_chest(&self, commands: &mut Commands) {
        AudioM::play_sfx_varied(commands, self.weapon_chest.clone(), 0.6, 0.05);
    }

    pub fn play_ammo_chest(&self, commands: &mut Commands) {
        AudioM::play_sfx_varied(commands, self.ammo_chest.clone(), 0.6, 0.05);
    }

    pub fn play_pickup_disappear(&self, commands: &mut Commands) {
        AudioM::play_sfx_varied(commands, self.pickup_disappear.clone(), 0.4, 0.1);
    }

    pub fn play_chest(&self, commands: &mut Commands) {
        AudioM::play_sfx_varied(commands, self.chest.clone(), 0.6, 0.05);
    }

    pub fn play_empty(&self, commands: &mut Commands) {
        AudioM::play_sfx_varied(commands, self.empty.clone(), 0.6, 0.05);
    }

    pub fn play_ultra_empty(&self, commands: &mut Commands) {
        AudioM::play_sfx_varied(commands, self.ultra_empty.clone(), 0.6, 0.05);
    }

    /// Per-family fire sound (GML scrFire snd table). Falls back to generic
    /// shoot/explode for families without a dedicated handle.
    pub fn play_weapon_fire(&self, commands: &mut Commands, weapon_name: &str) {
        let n = weapon_name;
        if n.contains("PLASMA") || n.contains("DEVASTATOR") || n == "GUN GUN" {
            AudioM::play_sfx_varied(commands, self.plasma.clone(), 0.55, 0.08);
        } else if n.contains("LASER") || n.contains("ION") {
            AudioM::play_sfx_varied(commands, self.laser.clone(), 0.55, 0.08);
        } else if n.contains("LIGHTNING") {
            AudioM::play_sfx_varied(commands, self.lightning.clone(), 0.55, 0.08);
        } else if n.contains("FLAME")
            || n.contains("DRAGON")
            || n.contains("FLARE")
            || n.contains("INCINERATOR")
        {
            AudioM::play_sfx_varied(commands, self.flame.clone(), 0.55, 0.08);
        } else if n.contains("DISC") || n.contains("BOUNCER") {
            AudioM::play_sfx_varied(commands, self.disc.clone(), 0.55, 0.08);
        } else if n.contains("SLUGGER") {
            AudioM::play_sfx_varied(commands, self.slugger.clone(), 0.6, 0.08);
        } else if n.contains("SPLINTER") || n.contains("SEEKER") || n.contains("TOXIC") {
            AudioM::play_sfx_varied(commands, self.splinter.clone(), 0.5, 0.08);
        } else if n.contains("GRENADE")
            || n.contains("BAZOOKA")
            || n.contains("NUKE")
            || n.contains("ROCKET")
            || n.contains("CLUSTER")
            || n.contains("BLOOD")
            || n.contains("FLAK")
            || n.contains("NADER")
        {
            AudioM::play_sfx_varied(commands, self.grenade.clone(), 0.6, 0.08);
        } else if n.contains("CROSSBOW") || n.contains("HEAVY XBOW") {
            self.play_bolt(commands);
        } else if n.contains("SHOTGUN")
            || n.contains("ERASER")
            || n.contains("WAVE")
            || n.contains("SLUGGER")
        {
            self.play_shotgun(commands);
        } else if n.contains("MACHINEGUN")
            || n.contains("SMG")
            || n.contains("MINIGUN")
            || n.contains("ASSAULT")
            || n.contains("QUAD")
            || n.contains("POP RIFLE")
            || n.contains("ROGUE")
            || n.contains("HEAVY")
        {
            self.play_machine(commands);
        } else if n.contains("REVOLVER")
            || n.contains("PISTOL")
            || n.contains("SMART")
            || n.contains("POP GUN")
            || n.contains("FROG")
        {
            self.play_shoot(commands);
        } else if n.contains("SENTRY") {
            self.play_machine(commands);
        } else {
            self.play_shoot(commands);
        }
    }
}
