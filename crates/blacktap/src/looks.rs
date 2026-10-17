//! Character looks and numbers per archetype, from `npcs.toml`.

use std::collections::BTreeMap;

use pav_core::params::ChoiceParam;
use pav_core::puppet::{BodyPlan, HelmKind, PuppetDef, WeaponKind};
use pav_kit::combat::CombatProfile;
use pav_kit::perception::Senses;
use serde::{Deserialize, Serialize};

/// How an archetype looks: colours sRGB hex; names as `pav_core::puppet` spells them. Every
/// field is optional in `npcs.toml` (the defaults below).
#[derive(Clone, Debug, Serialize, Deserialize)]
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
    /// The weapon's grip, haft or stock ("" = the puppet's wood).
    pub weapon_grip: String,
    pub scale: f32,
    /// Build: thicker torso, wider shoulders and stouter limbs (1 = ordinary; brutes 1.25).
    pub bulk: f32,
    /// Edging of hats and coats ("" = none; `GearLook::trim`).
    pub trim: String,
    /// Coat skirts from the waist (0 = none, 1 = to the knees) and their colour ("" = shirt).
    pub coat: f32,
    pub coat_color: String,
    /// Hair or wig colour ("" = none), shirt stripes ("" = none), the belt's strap ("" = none).
    pub hair: String,
    pub stripes: String,
    pub belt_strap: String,
    /// The Tap bracer's leather on the left forearm ("" = none).
    pub bracer: String,
    /// Pistols holstered at the belt (0..4).
    pub holsters: i32,
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
            weapon_grip: String::new(),
            scale: 1.0,
            bulk: 1.0,
            trim: String::new(),
            coat: 0.0,
            coat_color: String::new(),
            hair: String::new(),
            stripes: String::new(),
            belt_strap: String::new(),
            bracer: String::new(),
            holsters: 0,
        }
    }
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default)]
pub struct Archetype {
    pub look: LookDef,
    /// Variations on `look` for crowds (civilians, crew): each a partial look whose fields
    /// replace the base's (`{ shirt = "#..", helm = "straw_hat" }`); `look_variant` picks one.
    pub variants: Vec<toml::Table>,
    pub combat: CombatProfile,
    pub senses: Senses,
}

impl Archetype {
    /// The look for variant `i` (wrapping round; the base look when there are none).
    pub fn variant(&self, i: usize) -> LookDef {
        if self.variants.is_empty() {
            return self.look.clone();
        }
        let mut table = toml::Table::try_from(&self.look).unwrap_or_default();
        for (k, v) in &self.variants[i % self.variants.len()] {
            table.insert(k.clone(), v.clone());
        }
        table.try_into().unwrap_or_else(|e| {
            log::warn!("npcs.toml variant: {e}");
            self.look.clone()
        })
    }
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
        if self.bulk != 1.0 {
            let b = self.bulk.clamp(0.6, 1.8);
            p.torso_radius *= b;
            p.shoulder_width *= 1.0 + (b - 1.0) * 0.7;
            p.hip_width *= 1.0 + (b - 1.0) * 0.5;
            p.limb_radius *= 1.0 + (b - 1.0) * 0.8;
        }
        p.gear.helm = choice::<HelmKind>(&self.helm).unwrap_or_default();
        p.gear.helm_color = self.helm_color.clone();
        p.gear.pauldrons = self.pauldrons;
        p.gear.armor_color = self.armor_color.clone();
        p.gear.cape = self.cape;
        p.gear.cape_color = self.cape_color.clone();
        p.gear.gloves = self.gloves.clone();
        p.gear.belt = self.belt.clone();
        p.gear.trim = self.trim.clone();
        p.gear.coat = self.coat;
        p.gear.coat_color = self.coat_color.clone();
        p.gear.hair = self.hair.clone();
        p.gear.stripes = self.stripes.clone();
        p.gear.belt_strap = self.belt_strap.clone();
        p.gear.bracer = self.bracer.clone();
        p.gear.holsters = self.holsters;
        p.weapon.kind = choice::<WeaponKind>(&self.weapon).unwrap_or_default();
        p.weapon.color = self.weapon_color.clone();
        if !self.weapon_grip.is_empty() {
            p.weapon.grip = self.weapon_grip.clone();
        }
        p
    }

    /// Names this look uses that the puppet does not know (data checks).
    pub fn problems(&self) -> Vec<String> {
        let mut out = Vec::new();
        if choice::<HelmKind>(&self.helm).is_none() {
            out.push(format!("unknown helm '{}' (one of {})", self.helm, HelmKind::NAMES.join(", ")));
        }
        if choice::<WeaponKind>(&self.weapon).is_none() {
            out.push(format!("unknown weapon '{}' (one of {})", self.weapon, WeaponKind::NAMES.join(", ")));
        }
        out
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

/// The look of the `index`th character of an archetype in a crowd: one of its `variants`
/// (shirts, hats...), picked from the index so the same character always looks the same and
/// neighbours differ; the base look when it has none.
pub fn look_variant(kind: &str, index: u32) -> PuppetDef {
    let a = archetype(kind);
    if a.variants.is_empty() {
        return a.look.puppet();
    }
    let n = a.variants.len() as u64;
    // Each run of `n` indices meets every variant once, in an order scrambled per run.
    let run = pav_core::rng::hash3(0xb1ac_7a90, (index as u64 / n) as i32, 0, 0) % n;
    a.variant(((index as u64 + run) % n) as usize).puppet()
}

/// Every unknown name in every archetype's look and variants (data checks).
pub fn problems() -> Vec<String> {
    let file = archetypes();
    let mut out = Vec::new();
    for (name, a) in &file.archetype {
        let mut looks = vec![("look".to_string(), a.look.clone())];
        looks.extend((0..a.variants.len()).map(|i| (format!("variant {i}"), a.variant(i))));
        for (what, l) in looks {
            out.extend(l.problems().into_iter().map(|p| format!("archetype '{name}' {what}: {p}")));
        }
    }
    out
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_look_names_known_gear() {
        assert!(!names().is_empty());
        let problems = problems();
        assert!(problems.is_empty(), "{problems:#?}");
    }

    #[test]
    fn variants_are_stable_and_varied() {
        for kind in ["civilian", "crew"] {
            let looks: Vec<PuppetDef> = (0..12).map(|i| look_variant(kind, i)).collect();
            assert_eq!(looks[3], look_variant(kind, 3), "{kind}: same index, same look");
            let distinct = looks.iter().enumerate().filter(|(i, l)| !looks[..*i].contains(l)).count();
            assert!(distinct >= 3, "{kind}: {distinct} distinct looks in 12");
        }
        // An archetype without variants keeps its one look.
        assert_eq!(look_variant("guard", 5), look("guard"));
    }
}
