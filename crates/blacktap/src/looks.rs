//! Character looks and numbers per archetype, from `npcs.toml`.

use std::collections::BTreeMap;

use pav_core::params::ChoiceParam;
use pav_core::puppet::{BodyPlan, HelmKind, PuppetDef, WeaponKind};
use pav_kit::combat::CombatProfile;
use pav_kit::perception::Senses;
use serde::Deserialize;

#[derive(Clone, Debug, Deserialize)]
#[serde(default)]
pub struct LookDef {
    pub skin: String,
    pub shirt: String,
    pub pants: String,
    pub shoes: String,
    pub helm: String,
    pub helm_color: String,
    pub pauldrons: f32,
    pub armor_color: String,
    pub cape: f32,
    pub cape_color: String,
    pub gloves: String,
    pub belt: String,
    pub weapon: String,
    pub weapon_color: String,
    pub scale: f32,
}

impl Default for LookDef {
    fn default() -> Self {
        Self {
            skin: "#d2a682".into(),
            shirt: "#e8e8ee".into(),
            pants: "#2c3e66".into(),
            shoes: "#1a1a1a".into(),
            helm: "none".into(),
            helm_color: "#2c3e66".into(),
            pauldrons: 0.0,
            armor_color: "#8a8f99".into(),
            cape: 0.0,
            cape_color: "#3a2f4a".into(),
            gloves: String::new(),
            belt: String::new(),
            weapon: "none".into(),
            weapon_color: "#c9ced8".into(),
            scale: 1.0,
        }
    }
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default)]
pub struct Archetype {
    pub look: LookDef,
    pub combat: CombatProfile,
    pub senses: Senses,
}

#[derive(Debug, Default, Deserialize)]
pub struct NpcsFile {
    #[serde(default)]
    pub archetype: BTreeMap<String, Archetype>,
}

impl LookDef {
    pub fn puppet(&self) -> PuppetDef {
        let mut p = PuppetDef::preset(BodyPlan::Biped);
        p.skin = self.skin.clone();
        p.shirt = self.shirt.clone();
        p.pants = self.pants.clone();
        p.shoes = self.shoes.clone();
        p.scale *= self.scale;
        p.gear.helm = choice::<HelmKind>(&self.helm).unwrap_or_default();
        p.gear.helm_color = self.helm_color.clone();
        p.gear.pauldrons = self.pauldrons;
        p.gear.armor_color = self.armor_color.clone();
        p.gear.cape = self.cape;
        p.gear.cape_color = self.cape_color.clone();
        p.gear.gloves = self.gloves.clone();
        p.gear.belt = self.belt.clone();
        p.weapon.kind = choice::<WeaponKind>(&self.weapon).unwrap_or_default();
        p.weapon.color = self.weapon_color.clone();
        p
    }
}

/// A choice by its data name.
fn choice<T: ChoiceParam>(name: &str) -> Option<T> {
    T::NAMES.iter().position(|n| *n == name).map(T::from_index)
}

fn archetypes() -> std::sync::Arc<NpcsFile> {
    use std::sync::{Arc, OnceLock};
    static NPCS: OnceLock<Arc<NpcsFile>> = OnceLock::new();
    NPCS.get_or_init(|| {
        let text = crate::data("npcs.toml").unwrap_or_default();
        Arc::new(toml::from_str(&text).unwrap_or_else(|e| {
            log::warn!("npcs.toml: {e}");
            NpcsFile::default()
        }))
    })
    .clone()
}

/// An archetype by name (the default one when unknown).
pub fn archetype(kind: &str) -> Archetype {
    archetypes().archetype.get(kind).cloned().unwrap_or_default()
}

pub fn look(kind: &str) -> PuppetDef {
    archetype(kind).look.puppet()
}

pub fn profile(kind: &str) -> CombatProfile {
    archetype(kind).combat
}

pub fn senses(kind: &str) -> Senses {
    archetype(kind).senses
}

/// Captain Rowan Cask.
pub fn captain() -> PuppetDef {
    look("captain")
}

/// Every archetype's name (data checks).
pub fn names() -> Vec<String> {
    archetypes().archetype.keys().cloned().collect()
}
