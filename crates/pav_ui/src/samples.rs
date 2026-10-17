//! Rich sample data for galleries, captures and tests.

use std::sync::Arc;

use glam::{Vec2, Vec3};
use pav_core::play::{
    Awareness, AwarenessArc, Banner, Compass, CompassMark, DemoSegment, FrontEndInfo, HudItem, MapData, MapIcon, MarkerKind,
    Objective, Overlay, Prompt, Results, ScreenFx, Subtitle, Tile, Toast, UiModel, WeaponPip, WorldMarker,
};

use crate::Projector;

/// A camera that frames [`sample_ui_model`]'s markers (screen `size` in points).
pub fn sample_projector(size: egui::Vec2) -> Projector {
    Projector::look_at(Vec3::new(0.0, 14.0, 40.0), Vec3::new(0.0, 4.0, -30.0), 60.0, size)
}

fn tile(name: &str, desc: &str, cost: &str, icon: &str) -> Tile {
    Tile { name: name.into(), desc: desc.into(), cost: cost.into(), icon: icon.into(), disabled: false }
}

/// A sample HUD with every kind of item, markers, awareness, compass, subtitles, a banner,
/// toasts and a map (no overlay: see [`sample_overlays`]).
pub fn sample_ui_model() -> UiModel {
    UiModel {
        hud: vec![
            HudItem::Health { frac: 0.62, segments: 5, heat: 2 },
            HudItem::Counter { icon: "reales".into(), label: "Reales".into(), value: "1,240".into() },
            HudItem::Counter { icon: "timber".into(), label: "Timber".into(), value: "36".into() },
            HudItem::Ship {
                hull: 0.74,
                sails: 0.9,
                wind_rel: 125.0,
                wind: 0.7,
                sail: 2,
                knots: 9.4,
                weapons: vec![
                    WeaponPip { name: "port".into(), ready: 1.0, active: true },
                    WeaponPip { name: "starboard".into(), ready: 0.45, active: false },
                    WeaponPip { name: "chain".into(), ready: 1.0, active: false },
                    WeaponPip { name: "kegs".into(), ready: 0.2, active: false },
                    WeaponPip { name: "swivel".into(), ready: 0.8, active: false },
                ],
            },
            HudItem::Bar { label: "Fort Ventana".into(), frac: 0.38, color: "#a8322d".into() },
            HudItem::Tool { name: "Smoke Bomb".into(), count: 3 },
        ],
        objective: Some(Objective {
            title: "Second Round — Broadside".into(),
            text: "Sink the gunboats (2/3)".into(),
            optional: vec![
                ("Keep the hull above half".into(), true),
                ("Chain-shot a mast".into(), false),
                ("Board the brig".into(), false),
            ],
            age: 2.0,
        }),
        prompts: vec![
            Prompt::new("interact", "Take the helm"),
            Prompt::new("attack", "Assassinate"),
            Prompt { action: "drop".into(), label: "Dive".into(), hold: true },
        ],
        subtitles: vec![Subtitle {
            speaker: "Old Bung".into(),
            color: "#e2b04a".into(),
            text: "Steady, lad. Let the wind do the pouring — we'll do the tapping.".into(),
        }],
        banner: Some(Banner {
            text: "FORT CAPTURED".into(),
            sub: "Fort Ventana falls silent".into(),
            age: 1.2,
            color: "#e2b04a".into(),
        }),
        toasts: vec![
            Toast { text: "Shanty sheet found: Leave Her, Johnny (3/12)".into(), age: 1.0 },
            Toast { text: "Upgrade ready at the Black Tap".into(), age: 2.5 },
        ],
        markers: vec![
            WorldMarker {
                pos: Vec3::new(38.0, 8.0, -170.0),
                kind: MarkerKind::Objective,
                label: "Gunboat".into(),
                bar: None,
                distance: true,
            },
            WorldMarker {
                pos: Vec3::new(-70.0, 6.0, -140.0),
                kind: MarkerKind::Enemy,
                label: "Gunboat".into(),
                bar: Some(0.55),
                distance: false,
            },
            WorldMarker {
                pos: Vec3::new(14.0, 5.0, -62.0),
                kind: MarkerKind::Target,
                label: "Harbourmaster Quill".into(),
                bar: None,
                distance: true,
            },
            WorldMarker {
                pos: Vec3::new(5.0, 6.0, 12.0),
                kind: MarkerKind::Interact,
                label: "Helm".into(),
                bar: None,
                distance: false,
            },
            WorldMarker {
                pos: Vec3::new(-26.0, 3.0, -38.0),
                kind: MarkerKind::Collectible,
                label: "Bottle".into(),
                bar: None,
                distance: true,
            },
            WorldMarker {
                pos: Vec3::new(-40.0, 3.0, -60.0),
                kind: MarkerKind::Ally,
                label: "Mags".into(),
                bar: None,
                distance: false,
            },
            WorldMarker {
                pos: Vec3::new(10.0, 0.0, 140.0),
                kind: MarkerKind::Waypoint,
                label: "".into(),
                bar: None,
                distance: true,
            },
        ],
        awareness: vec![
            AwarenessArc { pos: Vec3::new(-14.0, 6.0, -44.0), fill: 0.5, state: Awareness::Suspicious },
            AwarenessArc { pos: Vec3::new(28.0, 7.0, -78.0), fill: 1.0, state: Awareness::Alerted },
            AwarenessArc { pos: Vec3::new(8.0, 5.5, -22.0), fill: 0.35, state: Awareness::Unaware },
            AwarenessArc { pos: Vec3::new(-6.0, 4.0, 110.0), fill: 0.8, state: Awareness::Searching },
        ],
        compass: Some(Compass {
            heading: 20.0,
            marks: vec![
                CompassMark { bearing: 32.0, kind: MarkerKind::Objective, distance: 172.0 },
                CompassMark { bearing: 340.0, kind: MarkerKind::Collectible, distance: 64.0 },
                CompassMark { bearing: 75.0, kind: MarkerKind::Waypoint, distance: 1340.0 },
                CompassMark { bearing: 5.0, kind: MarkerKind::Enemy, distance: 150.0 },
                CompassMark { bearing: 48.0, kind: MarkerKind::Threat, distance: 40.0 },
                CompassMark { bearing: 200.0, kind: MarkerKind::Target, distance: 300.0 },
            ],
        }),
        overlay: None,
        demo: None,
        screen: ScreenFx { vignette: 0.22, ..Default::default() },
        request: None,
        map: Some(Arc::new(sample_map())),
        map_icons: sample_map_icons(),
    }
}

fn blob(center: Vec2, radius: f32, seed: u32, wobble: f32, n: usize) -> Vec<Vec2> {
    let mut pts: Vec<Vec2> = (0..n)
        .map(|i| {
            let a = i as f32 / n as f32 * std::f32::consts::TAU;
            let r = radius
                * (1.0
                    + wobble * (crate::paint::noise(i as f32 * 0.35, seed) - 0.5) * 2.0
                    + 0.12 * (a * 3.0 + seed as f32).sin());
            center + Vec2::new(a.cos(), a.sin()) * r
        })
        .collect();
    pts.push(pts[0]);
    pts
}

/// A sample chart: an island with a contour, a town, roads and labels.
pub fn sample_map() -> MapData {
    let coast = blob(Vec2::new(-40.0, 20.0), 230.0, 7, 0.35, 96);
    let hill = blob(Vec2::new(-80.0, 40.0), 110.0, 11, 0.3, 64);
    let islet = blob(Vec2::new(250.0, -230.0), 60.0, 3, 0.3, 40);
    let mut fills = Vec::new();
    for (i, (x, z)) in
        [(40.0f32, -60.0f32), (64.0, -50.0), (52.0, -90.0), (80.0, -82.0), (30.0, -100.0), (96.0, -60.0)].iter().enumerate()
    {
        let (w, h) = (16.0 + (i % 3) as f32 * 5.0, 12.0 + (i % 2) as f32 * 6.0);
        fills.push((
            vec![Vec2::new(*x, *z), Vec2::new(x + w, *z), Vec2::new(x + w, z + h), Vec2::new(*x, z + h)],
            "#6b4526".to_string(),
        ));
    }
    // The fort: an L-shaped (concave) footprint.
    fills.push((
        vec![
            Vec2::new(-170.0, -120.0),
            Vec2::new(-120.0, -120.0),
            Vec2::new(-120.0, -100.0),
            Vec2::new(-150.0, -100.0),
            Vec2::new(-150.0, -70.0),
            Vec2::new(-170.0, -70.0),
        ],
        "#5a4330".to_string(),
    ));
    MapData {
        min: Vec2::new(-400.0, -400.0),
        max: Vec2::new(400.0, 400.0),
        lines: vec![
            (coast, "#2b1d12".into(), 3.0),
            (hill, "#7a5a3a".into(), 1.5),
            (islet, "#2b1d12".into(), 3.0),
            (
                vec![Vec2::new(60.0, -70.0), Vec2::new(0.0, -20.0), Vec2::new(-60.0, 30.0), Vec2::new(-90.0, 60.0)],
                "#8a6a44".into(),
                2.5,
            ),
        ],
        fills,
        labels: vec![
            (Vec2::new(-60.0, 90.0), "Isla Barrica".into()),
            (Vec2::new(60.0, -125.0), "Puerto Grifo".into()),
            (Vec2::new(-150.0, -140.0), "Fort Ventana".into()),
            (Vec2::new(250.0, -310.0), "Cayo Corcho".into()),
        ],
        fog_size: 0,
    }
}

fn sample_map_icons() -> Vec<MapIcon> {
    let i = |x: f32, z: f32, icon: &str, yaw: f32| MapIcon { at: Vec2::new(x, z), icon: icon.into(), yaw };
    vec![
        i(140.0, 40.0, "player", 300.0),
        i(-150.0, -95.0, "fort", 0.0),
        i(70.0, -70.0, "tavern", 0.0),
        i(-90.0, 30.0, "perch", 0.0),
        i(10.0, 130.0, "perch_done", 0.0),
        i(-20.0, -10.0, "chest", 0.0),
        i(-130.0, 120.0, "sheet", 0.0),
        i(250.0, -230.0, "bottle", 0.0),
        i(200.0, 150.0, "enemy_ship", 0.0),
        i(-260.0, -220.0, "objective", 0.0),
        i(310.0, 60.0, "flotsam", 0.0),
    ]
}

/// One of each overlay, named.
pub fn sample_overlays() -> Vec<(&'static str, Overlay)> {
    vec![
        (
            "tiles",
            Overlay::Tiles {
                title: "Plunder".into(),
                subtitle: "Choose one prize from the gunboat's hold".into(),
                tiles: vec![
                    tile("Oak Planking", "Hull +20%. Shrugs off a broadside.", "120 reales", "hull"),
                    tile("Chain Shot", "Tears sails: enemies slow to a crawl.", "90 reales", "chain"),
                    tile("Powder Kegs", "Drop a keg astern. Mind the wake.", "150 reales", "kegs"),
                ],
                focus: 1,
            },
        ),
        (
            "wheel",
            Overlay::Wheel {
                slots: vec![
                    tile("Pistol", "Two shots", "×2", "pistol"),
                    tile("Smoke Bomb", "Blind and slip away", "×3", "smoke"),
                    tile("Throwing Knife", "Silent at range", "×5", "dart"),
                    tile("Grog Bottle", "Lure the thirsty", "×1", "grog"),
                ],
                selected: 1,
            },
        ),
        (
            "results",
            Overlay::Results(Results {
                title: "Second Round — Broadside".into(),
                pour: 80.0,
                checklist: vec![
                    ("Sink all three gunboats".into(), true),
                    ("Keep the hull above half".into(), true),
                    ("Chain-shot a mast".into(), true),
                    ("Board the brig".into(), true),
                    ("Never seen by the fort".into(), false),
                ],
                stats: vec![
                    ("Time".into(), "7:42".into()),
                    ("Reales".into(), "+640".into()),
                    ("Collectibles".into(), "4 / 6".into()),
                    ("Assassinations".into(), "3".into()),
                    ("Ships sunk".into(), "3".into()),
                    ("Times detected".into(), "1".into()),
                ],
                tiles: vec![
                    tile("Free Roam", "", "", "sail"),
                    tile("Replay a Round", "", "", "map"),
                    tile("Title", "", "", "keg"),
                ],
                focus: 0,
            }),
        ),
        ("map", Overlay::Map { cursor: Vec2::new(60.0, -40.0), zoom: 1.6 }),
        ("cinematic", Overlay::Cinematic { skip: 0.4 }),
    ]
}

/// A front-end description like the game's (for tests, galleries and tools).
pub fn sample_front_end_info() -> FrontEndInfo {
    let seg = |name: &str, scene: &str, caption: &str, seconds: f32| DemoSegment {
        name: name.into(),
        scene: scene.into(),
        caption: caption.into(),
        seeds: vec![11, 22, 33],
        seconds,
    };
    FrontEndInfo {
        title: "Assassin's Keg — Black Tap".into(),
        tagline: "A pirate's last pour".into(),
        id: "AssassinsKegBlackTap-W9".into(),
        title_scene: "blacktap/title".into(),
        new_game_scene: "blacktap/round/1".into(),
        training_scene: Some("blacktap/drydock".into()),
        chapters: vec![
            ("I · The Dry Run".into(), "blacktap/round/1".into()),
            ("II · Broadside".into(), "blacktap/round/2".into()),
            ("III · Storm the Fort".into(), "blacktap/round/3".into()),
            ("IV · Ashore".into(), "blacktap/round/4".into()),
            ("V · Shadow of the Keg".into(), "blacktap/round/5".into()),
            ("VI · Last Call".into(), "blacktap/round/6".into()),
        ],
        demo: vec![
            seg("Rule the Waves", "blacktap/round/2", "Rule the Waves — broadsides, chain shot and a boarding brawl", 75.0),
            seg("Storm the Fort", "blacktap/round/3", "Storm the Fort — dodge the mortars, silence the batteries", 50.0),
            seg("Ashore", "blacktap/round/4", "Ashore — dive, climb the chapel, take the leap", 60.0),
            seg("Shadow of the Keg", "blacktap/round/5", "Shadow of the Keg — Gull Sight, tall grass and quiet blades", 70.0),
            seg("Last Call", "blacktap/round/6", "Last Call — over the rooftops and into the storm", 60.0),
        ],
        credits: vec![
            ("Assassin's Keg: Black Tap".into(), "A pirate stealth-and-sail adventure made with the Pavilion engine (shardfall). Everything you see and hear is procedural: ships, islands, sea, sky, music and sound are generated from code and data.".into()),
            ("Engine".into(), "Pavilion, the agent-first engine from shardfall. Rendering through wgpu (WebGPU), physics by Rapier, UI by egui.".into()),
            ("Fonts".into(), "Pirata One by Rodrigo Fuenzalida and Alegreya Sans by Juan Pablo del Peral (Huerta Tipográfica), both under the SIL Open Font License 1.1.".into()),
            ("Music".into(), "Traditional sea shanties in the public domain, arranged procedurally: Leave Her, Johnny; Drunken Sailor; Haul Away Joe; South Australia; Spanish Ladies.".into()),
            ("Source Information".into(), "An homage to the open-sea assassin games of the 2010s: naval combat, free-running and stealth in the age of sail, reimagined around a rum keg and its tap. No assets or code from any other game are used.".into()),
            ("Thanks".into(), "To everyone who ever sang a shanty off-key on a Friday night.".into()),
        ],
        theme: String::new(),
        bindings: String::new(),
        banks: Vec::new(),
        songs: Vec::new(),
        save_key: "blacktap".into(),
    }
}
