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

    pub weapon_chest: Handle<AudioSource>,
    pub ammo_chest: Handle<AudioSource>,

    pub pickup_disappear: Handle<AudioSource>,

    pub empty: Handle<AudioSource>,
    pub ultra_empty: Handle<AudioSource>,

    pub plasma: Handle<AudioSource>,
    pub laser: Handle<AudioSource>,
    pub lightning: Handle<AudioSource>,
    pub flame: Handle<AudioSource>,
    pub disc: Handle<AudioSource>,
    pub slugger: Handle<AudioSource>,
    pub grenade: Handle<AudioSource>,
    pub splinter: Handle<AudioSource>,

    pub gold_pistol: Handle<AudioSource>,
    pub gold_machinegun: Handle<AudioSource>,
    pub gold_shotgun: Handle<AudioSource>,
    pub gold_crossbow: Handle<AudioSource>,
    pub gold_grenade: Handle<AudioSource>,
    pub gold_plasma: Handle<AudioSource>,
    pub gold_laser: Handle<AudioSource>,
    pub melee_flip: Handle<AudioSource>,
    pub cross_reload: Handle<AudioSource>,
    pub shot_reload: Handle<AudioSource>,
    pub nade_reload: Handle<AudioSource>,
    pub plasma_reload: Handle<AudioSource>,
    pub lightning_reload: Handle<AudioSource>,
    pub oasis_shoot: Handle<AudioSource>,
    pub sniper_target: Handle<AudioSource>,
    pub sniper_fire: Handle<AudioSource>,
    pub assassin_attack: Handle<AudioSource>,
    pub laser_charge: Handle<AudioSource>,
    pub lightning_charge: Handle<AudioSource>,
    pub snowtank_aim: Handle<AudioSource>,
    pub goldtank_aim: Handle<AudioSource>,
    pub explo_charge: Handle<AudioSource>,
    pub mimic_slurp: Handle<AudioSource>,
    pub van_warning: Handle<AudioSource>,
    pub ultra_grenade: Handle<AudioSource>,
    pub heavy_nader: Handle<AudioSource>,
}

fn resolve_sfx(catalog: &AssetCatalog, stem: &str) -> String {
    for dir in ["audio", "sounds"] {
        for ext in ["ogg", "wav", "mp3", "flac"] {
            let path = format!("{dir}/{stem}.{ext}");
            if catalog.has_audio(&path) {
                return path;
            }
        }
    }

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
            gold_pistol: load_sfx(asset_server, catalog, "sndGoldPistol"),
            gold_machinegun: load_sfx(asset_server, catalog, "sndGoldMachinegun"),
            gold_shotgun: load_sfx(asset_server, catalog, "sndDoubleShotgun"),
            gold_crossbow: load_sfx(asset_server, catalog, "sndGoldCrossbow"),
            gold_grenade: load_sfx(asset_server, catalog, "sndGoldGrenade"),
            gold_plasma: load_sfx(asset_server, catalog, "sndGoldPlasma"),
            gold_laser: load_sfx(asset_server, catalog, "sndGoldLaser"),
            melee_flip: load_sfx(asset_server, catalog, "sndMeleeFlip"),
            cross_reload: load_sfx(asset_server, catalog, "sndCrossReload"),
            shot_reload: load_sfx(asset_server, catalog, "sndShotReload"),
            nade_reload: load_sfx(asset_server, catalog, "sndNadeReload"),
            plasma_reload: load_sfx(asset_server, catalog, "sndPlasmaReload"),
            lightning_reload: load_sfx(asset_server, catalog, "sndLightningReload"),
            oasis_shoot: load_sfx(asset_server, catalog, "sndOasisShoot"),
            sniper_target: load_sfx(asset_server, catalog, "sndSniperTarget"),
            sniper_fire: load_sfx(asset_server, catalog, "sndSniperFire"),
            assassin_attack: load_sfx(asset_server, catalog, "sndAssassinAttack"),
            laser_charge: load_sfx(asset_server, catalog, "sndLaserCrystalCharge"),
            lightning_charge: load_sfx(asset_server, catalog, "sndLightningCrystalCharge"),
            snowtank_aim: load_sfx(asset_server, catalog, "sndSnowTankAim"),
            goldtank_aim: load_sfx(asset_server, catalog, "sndGoldTankAim"),
            explo_charge: load_sfx(asset_server, catalog, "sndExploGuardianCharge"),
            mimic_slurp: load_sfx(asset_server, catalog, "sndMimicSlurp"),
            van_warning: load_sfx(asset_server, catalog, "sndVanWarning"),
            ultra_grenade: load_sfx(asset_server, catalog, "sndUltraGrenade"),
            heavy_nader: load_sfx(asset_server, catalog, "sndHeavyNader"),
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

    pub fn play_melee_flip(&self, commands: &mut Commands) {
        AudioM::play_sfx_varied(commands, self.melee_flip.clone(), 0.5, 0.08);
    }

    pub fn play_cross_reload(&self, commands: &mut Commands) {
        AudioM::play_sfx_varied(commands, self.cross_reload.clone(), 0.5, 0.08);
    }

    pub fn play_shot_reload(&self, commands: &mut Commands) {
        AudioM::play_sfx_varied(commands, self.shot_reload.clone(), 0.5, 0.08);
    }

    pub fn play_nade_reload(&self, commands: &mut Commands) {
        AudioM::play_sfx_varied(commands, self.nade_reload.clone(), 0.5, 0.08);
    }

    pub fn play_plasma_reload(&self, commands: &mut Commands) {
        AudioM::play_sfx_varied(commands, self.plasma_reload.clone(), 0.5, 0.08);
    }

    pub fn play_lightning_reload(&self, commands: &mut Commands) {
        AudioM::play_sfx_varied(commands, self.lightning_reload.clone(), 0.5, 0.08);
    }

    pub fn play_sniper_target(&self, commands: &mut Commands) {
        AudioM::play_sfx_varied(commands, self.sniper_target.clone(), 0.6, 0.05);
    }

    pub fn play_sniper_fire(&self, commands: &mut Commands) {
        AudioM::play_sfx_varied(commands, self.sniper_fire.clone(), 0.5, 0.08);
    }

    pub fn play_assassin_attack(&self, commands: &mut Commands) {
        AudioM::play_sfx_varied(commands, self.assassin_attack.clone(), 0.6, 0.05);
    }

    pub fn play_laser_charge(&self, commands: &mut Commands) {
        AudioM::play_sfx_varied(commands, self.laser_charge.clone(), 0.5, 0.05);
    }

    pub fn play_lightning_charge(&self, commands: &mut Commands) {
        AudioM::play_sfx_varied(commands, self.lightning_charge.clone(), 0.5, 0.05);
    }

    pub fn play_snowtank_aim(&self, commands: &mut Commands) {
        AudioM::play_sfx_varied(commands, self.snowtank_aim.clone(), 0.6, 0.05);
    }

    pub fn play_goldtank_aim(&self, commands: &mut Commands) {
        AudioM::play_sfx_varied(commands, self.goldtank_aim.clone(), 0.6, 0.05);
    }

    pub fn play_explo_charge(&self, commands: &mut Commands) {
        AudioM::play_sfx_varied(commands, self.explo_charge.clone(), 0.6, 0.05);
    }

    pub fn play_mimic_slurp(&self, commands: &mut Commands) {
        AudioM::play_sfx_varied(commands, self.mimic_slurp.clone(), 0.6, 0.05);
    }

    pub fn play_van_warning(&self, commands: &mut Commands) {
        AudioM::play_sfx_varied(commands, self.van_warning.clone(), 0.7, 0.05);
    }

    /// GML snd_play_gun tiers: main 0.3, small 0.6, big 0.33 (all pitch ±0.1).
    /// Underwater (Oasis) overrides to sndOasisShoot.
    pub fn play_weapon_fire_gml(
        &self,
        commands: &mut Commands,
        weapon_name: &str,
        underwater: bool,
    ) {
        if underwater {
            AudioM::play_sfx_varied(commands, self.oasis_shoot.clone(), 0.5, 0.1);
            return;
        }
        let is_gold = weapon_name.contains("GOLDEN")
            || weapon_name.contains("GOLD ")
            || weapon_name.starts_with("GOLD");
        if is_gold {
            let n = weapon_name;
            if n.contains("PISTOL") || n.contains("REVOLVER") {
                AudioM::play_sfx_varied(commands, self.gold_pistol.clone(), 0.5, 0.1);
                return;
            } else if n.contains("MACHINEGUN") || n.contains("SMG") || n.contains("MINIGUN") {
                AudioM::play_sfx_varied(commands, self.gold_machinegun.clone(), 0.5, 0.1);
                return;
            } else if n.contains("SHOTGUN") || n.contains("ERASER") {
                AudioM::play_sfx_varied(commands, self.gold_shotgun.clone(), 0.5, 0.1);
                return;
            } else if n.contains("CROSSBOW") || n.contains("XBOW") {
                AudioM::play_sfx_varied(commands, self.gold_crossbow.clone(), 0.5, 0.1);
                return;
            } else if n.contains("GRENADE")
                || n.contains("ROCKET")
                || n.contains("NUKE")
                || n.contains("BAZOOKA")
            {
                AudioM::play_sfx_varied(commands, self.gold_grenade.clone(), 0.5, 0.1);
                return;
            } else if n.contains("PLASMA") {
                AudioM::play_sfx_varied(commands, self.gold_plasma.clone(), 0.5, 0.1);
                return;
            } else if n.contains("LASER") || n.contains("ION") {
                AudioM::play_sfx_varied(commands, self.gold_laser.clone(), 0.5, 0.1);
                return;
            }
        }
        self.play_weapon_fire(commands, weapon_name);
    }

    pub fn play_weapon_fire(&self, commands: &mut Commands, weapon_name: &str) {
        let n = weapon_name;
        if n == "ULTRA GRENADE LAUNCHER" {
            AudioM::play_sfx_varied(commands, self.ultra_grenade.clone(), 0.5, 0.08);
            return;
        } else if n == "HEAVY GRENADE LAUNCHER" {
            AudioM::play_sfx_varied(commands, self.heavy_nader.clone(), 0.5, 0.08);
            return;
        }
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
