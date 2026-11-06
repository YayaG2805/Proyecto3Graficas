//! OA, el planeta de Linterna Verde: todo lo que lo hace verse vivo (Diego).
//!
//!   1. BATERÍA CENTRAL DE PODER: la linterna gigante (assets/props/bateria_oa.obj) parada en la
//!      superficie, con un aura verde que pulsa.
//!   2. TORRES DE LOS GUARDIANES (assets/props/torre_oa.obj) repartidas con una semilla fija.
//!   3. ANILLO DE ENERGÍA alrededor del planeta: geometría generada aquí (un anillo plano),
//!      con energía que fluye a lo largo de él.
//!   4. PATRULLA DE LINTERNAS: varias Linternas volando en formación de V con estelas.
//!   5. AURORAS en los dos polos: cortinas de luz que ondulan.
//!
//! Lo opaco (batería, torres, patrulla) se dibuja con `draw`; lo que es luz (anillo, auroras,
//! aura de la batería) con `draw_glow`, DESPUÉS de todo lo opaco.
//! Los modelos se arman con assets/props/modelar_oa.py en Blender.

use crate::creatures::{LanternShader, TrailShader};
use crate::math::{Mat4, Vec3};
use crate::mesh::{Mesh, Vertex};
use crate::noise::fbm;
use crate::props::{instance_model, load_prop_lods, scatter, PropInstance};
use crate::renderer::{Fragment, FragmentShader, Renderer, Uniforms};
use crate::scene::Scene;
use std::f32::consts::{PI, TAU};
use std::sync::Arc;

// --- Batería Central ---
/// Dónde está la batería (dirección desde el centro de Oa, en su marco). Queda junto al
/// punto sobre el que flota Linterna Verde (LANTERN_SPOT en creatures.rs).
const BATTERY_SPOT: Vec3 = Vec3::new(0.65, 0.75, -0.05);
/// Alto de la batería en unidades del mundo (Oa tiene radio 4.5).
const BATTERY_HEIGHT: f32 = 1.4;
/// Tamaño del aura de la batería (en múltiplos de su alto) y velocidad del pulso.
const BATTERY_AURA: f32 = 1.3;
const BATTERY_PULSE_SPEED: f32 = 2.0;

// --- Torres de los Guardianes ---
/// Semilla, cantidad y alto de las torres. Cambiar la semilla cambia dónde quedan.
pub const TOWER_SEED: u32 = 4242;
const TOWER_COUNT: usize = 45;
const TOWER_HEIGHT: f32 = 1.2;
/// No hay torres cerca de la batería ni en los polos (ahí están las auroras).
const TOWER_BATTERY_CLEARANCE: f32 = 0.985; // coseno del ángulo mínimo a la batería
const TOWER_POLAR_LIMIT: f32 = 0.8; // |y| máximo de la dirección

// --- Anillo de energía ---
/// Radios interior y exterior del anillo (en múltiplos del radio de Oa) e inclinación.
const RING_INNER: f32 = 1.6;
const RING_OUTER: f32 = 2.5;
const RING_TILT: f32 = 0.35;
/// Qué tan rápido fluye la energía y cuántas franjas hay alrededor.
const RING_FLOW_SPEED: f32 = 0.6;
const RING_STREAKS: f32 = 18.0;
const RING_BRIGHTNESS: f32 = 0.55;

// --- Patrulla de Linternas ---
/// Cuántas Linternas, a qué distancia vuelan (radios de Oa), su tamaño y su velocidad.
const PATROL_COUNT: usize = 5;
const PATROL_RADII: f32 = 3.0;
const PATROL_SIZE: f32 = 0.35;
const PATROL_SPEED: f32 = 0.35; // radianes por segundo
/// Inclinación del plano de vuelo y separación de la V (radianes hacia atrás y unidades al lado).
const PATROL_TILT: f32 = 1.0;
const PATROL_GAP_BACK: f32 = 0.06;
const PATROL_GAP_SIDE: f32 = 0.9;
/// Estela: puntos, cada cuántos segundos, y color.
const PATROL_TRAIL_POINTS: usize = 16;
const PATROL_TRAIL_STEP: f32 = 0.12;
const PATROL_TRAIL: Vec3 = Vec3::new(0.2, 1.0, 0.4);

// --- Auroras ---
/// Latitud de las auroras (ángulo desde el polo), alto de la cortina (radios) y brillo.
const AURORA_COLATITUDE: f32 = 0.42;
const AURORA_HEIGHT: f32 = 0.35;
const AURORA_BRIGHTNESS: f32 = 0.6;
const AURORA_SPEED: f32 = 0.7;

// --- Colores de Oa ---
const OA_GREEN: Vec3 = Vec3::new(0.15, 1.0, 0.35);
const OA_TEAL: Vec3 = Vec3::new(0.2, 0.9, 0.8);
/// Paletas de los modelos (mismo orden que en modelar_oa.py): metal, brillo, adorno.
const BATTERY_PALETTE: [Vec3; 3] = [Vec3::new(0.06, 0.10, 0.08), Vec3::new(0.15, 1.0, 0.35), Vec3::new(0.55, 0.95, 0.55)];
const TOWER_PALETTE: [Vec3; 3] = [Vec3::new(0.08, 0.13, 0.11), Vec3::new(0.20, 1.0, 0.40), Vec3::new(0.80, 1.0, 0.80)];

pub struct Oa {
    /// Índice de Oa en scene.bodies.
    home: usize,
    battery: Vec<Mesh>,
    towers: Vec<Mesh>,
    tower_list: Vec<PropInstance>,
    lantern: Vec<Mesh>,
    ring: Mesh,
    aurora: Mesh,
    aura: Mesh,
}

impl Oa {
    /// Arma todo lo de Oa. Devuelve None si Oa no está en la escena.
    pub fn new(scene: &Scene) -> Option<Oa> {
        let home = scene.bodies.iter().position(|b| b.name == "Oa")?;
        let fallback = || vec![Mesh::icosphere(1, &|_| 1.0)];
        let battery = load_prop_lods("bateria_oa", BATTERY_HEIGHT).unwrap_or_else(fallback);
        let towers = load_prop_lods("torre_oa", TOWER_HEIGHT).unwrap_or_else(fallback);
        let spot = BATTERY_SPOT.normalized();
        let tower_list = scatter(TOWER_SEED, TOWER_COUNT, 0, &|d| d.dot(spot) < TOWER_BATTERY_CLEARANCE && d.y.abs() < TOWER_POLAR_LIMIT);
        // La patrulla usa el mismo modelo que Linterna Verde (con sus LODs).
        let lantern: Vec<Mesh> = ["", "_lod1", "_lod2"]
            .iter()
            .filter_map(|s| Mesh::load_obj(&format!("assets/heroes/linterna_verde{s}.obj"), Some(1.0)).ok())
            .collect();
        Some(Oa {
            home,
            battery,
            towers,
            tower_list,
            lantern: if lantern.is_empty() { fallback() } else { lantern },
            ring: ring_mesh(64),
            aurora: aurora_mesh(64),
            aura: Mesh::icosphere(2, &|_| 1.0),
        })
    }

    /// Marco de la batería: parada en BATTERY_SPOT sobre la superficie, gira con Oa.
    fn battery_model(&self, scene: &Scene, t: f32) -> Mat4 {
        let b = &scene.bodies[self.home];
        let dir = BATTERY_SPOT.normalized();
        let p = PropInstance { kind: 0, dir, scale: 1.0, yaw: 0.0 };
        instance_model(&scene.body_frame(self.home, t), b.radius * (b.surface)(dir), &p)
    }

    /// Dónde está la base de la batería y hacia dónde es "arriba" ahí (para las capturas).
    pub fn battery_spot(&self, scene: &Scene, t: f32) -> (Vec3, Vec3) {
        let m = self.battery_model(scene, t);
        (m.transform_point(Vec3::ZERO), m.transform_dir(Vec3::UP).normalized())
    }

    /// Marco del plano de vuelo de la patrulla: centrado en Oa, inclinado y SIN el giro del
    /// planeta (vuelan por su cuenta). Medido en unidades del mundo.
    fn patrol_frame(&self, scene: &Scene, t: f32) -> Mat4 {
        Mat4::translation(scene.body_position(self.home, t)) * Mat4::rotation_x(PATROL_TILT)
    }

    /// Posición de la Linterna `i` de la patrulla en el plano de vuelo, en el tiempo t.
    /// La líder (i = 0) va adelante; las demás se abren en V a los lados y un poco atrás.
    fn patrol_point(&self, scene: &Scene, i: usize, t: f32) -> Vec3 {
        let r = PATROL_RADII * scene.bodies[self.home].radius;
        let row = ((i + 1) / 2) as f32; // 0, 1, 1, 2, 2...
        let side = if i % 2 == 1 { 1.0 } else { -1.0 } * row * PATROL_GAP_SIDE;
        let a = PATROL_SPEED * t - row * PATROL_GAP_BACK;
        Vec3::new(a.cos() * r, side, a.sin() * r)
    }

    /// Lo opaco: batería, torres y la patrulla (con sus estelas).
    pub fn draw(&self, r: &mut Renderer, scene: &Scene, t: f32) {
        r.draw_lods(&self.battery, &self.battery_model(scene, t), Arc::new(OaPropShader { palette: BATTERY_PALETTE }));

        let b = &scene.bodies[self.home];
        let frame = scene.body_frame(self.home, t);
        let tower_shader: Arc<dyn FragmentShader> = Arc::new(OaPropShader { palette: TOWER_PALETTE });
        for p in &self.tower_list {
            let model = instance_model(&frame, b.radius * (b.surface)(p.dir), p);
            r.draw_lods(&self.towers, &model, tower_shader.clone());
        }

        let fly = self.patrol_frame(scene, t);
        for i in 0..PATROL_COUNT {
            let pos = self.patrol_point(scene, i, t);
            let ahead = self.patrol_point(scene, i, t + 0.05);
            // "Arriba" = hacia afuera del planeta; rotation_x(−90°) acuesta al modelo para que
            // vuele de cabeza hacia adelante (como Superman), con la cara hacia el planeta.
            let place = oriented(fly.transform_point(pos), fly.transform_dir(pos), fly.transform_dir(ahead - pos));
            let model = place * Mat4::rotation_x(-PI / 2.0) * Mat4::scale(Vec3::splat(PATROL_SIZE));
            r.draw_lods(&self.lantern, &model, Arc::new(LanternShader));
            // Estela: posiciones anteriores de la misma Linterna.
            let points: Vec<Vec3> =
                (0..PATROL_TRAIL_POINTS).map(|k| fly.transform_point(self.patrol_point(scene, i, t - k as f32 * PATROL_TRAIL_STEP))).collect();
            let length: f32 = points.windows(2).map(|w| (w[1] - w[0]).length()).sum();
            r.draw_line_strip(&points, Arc::new(TrailShader { color: PATROL_TRAIL, length }));
        }
    }

    /// Lo que es luz (aditivo): anillo, auroras y el aura de la batería.
    pub fn draw_glow(&self, r: &mut Renderer, scene: &Scene, t: f32) {
        let b = &scene.bodies[self.home];
        let center = scene.body_position(self.home, t);
        // Anillo: inclinado como el planeta y un poco más; no gira con él (la energía fluye en el shader).
        let ring_model = Mat4::translation(center)
            * Mat4::rotation_z(b.tilt)
            * Mat4::rotation_x(RING_TILT)
            * Mat4::scale(Vec3::splat(b.radius));
        r.draw_mesh_additive(&self.ring, &ring_model, Arc::new(RingShader));

        // Auroras: en el marco del planeta (giran con él), a la escala de su radio.
        let aurora_model = scene.body_frame(self.home, t) * Mat4::scale(Vec3::splat(b.radius));
        r.draw_mesh_additive(&self.aurora, &aurora_model, Arc::new(AuroraShader));

        // Aura de la batería: una esfera de luz en el centro del núcleo.
        let core = self.battery_model(scene, t).transform_point(Vec3::new(0.0, BATTERY_HEIGHT * 0.45, 0.0));
        let aura_model = Mat4::translation(core) * Mat4::scale(Vec3::splat(BATTERY_HEIGHT * BATTERY_AURA));
        r.draw_mesh_additive(&self.aura, &aura_model, Arc::new(BatteryAuraShader));
    }
}

/// Igual que en creatures.rs: mira hacia `forward` con su "arriba" hacia `up`, en `pos`.
fn oriented(pos: Vec3, up: Vec3, forward: Vec3) -> Mat4 {
    let up = up.normalized();
    let forward = (forward - up * forward.dot(up)).normalized();
    let right = forward.cross(up).normalized();
    Mat4::from_basis(right, up, forward, pos)
}

/// Agrega un cuadrilátero (a, b, c, d en orden) con sus DOS caras, para que se vea desde
/// ambos lados aunque el renderer descarte las caras traseras.
fn add_quad_both_sides(m: &mut Mesh, corners: [(Vec3, (f32, f32)); 4]) {
    let base = m.vertices.len() as u32;
    for (position, uv) in corners {
        m.vertices.push(Vertex { position, normal: Vec3::ZERO, uv, tangent: Vec3::ZERO });
    }
    m.indices.extend([base, base + 1, base + 2, base, base + 2, base + 3]); // una cara
    m.indices.extend([base, base + 2, base + 1, base, base + 3, base + 2]); // la otra
}

/// Anillo plano en el plano XZ, de RING_INNER a RING_OUTER (radios del planeta = 1).
fn ring_mesh(segments: usize) -> Mesh {
    let mut m = Mesh::default();
    for j in 0..segments {
        let (a0, a1) = (j as f32 / segments as f32 * TAU, (j + 1) as f32 / segments as f32 * TAU);
        let p = |a: f32, r: f32| Vec3::new(a.cos() * r, 0.0, a.sin() * r);
        add_quad_both_sides(
            &mut m,
            [
                (p(a0, RING_INNER), (0.0, 0.0)),
                (p(a0, RING_OUTER), (0.0, 1.0)),
                (p(a1, RING_OUTER), (0.0, 1.0)),
                (p(a1, RING_INNER), (0.0, 0.0)),
            ],
        );
    }
    m.recompute_normals();
    m.compute_bounds();
    m
}

/// Cortinas de aurora: una banda vertical alrededor de cada polo (radio del planeta = 1).
/// uv.1 = 0 abajo (cerca del suelo) y 1 arriba.
fn aurora_mesh(segments: usize) -> Mesh {
    let mut m = Mesh::default();
    for pole in [1.0f32, -1.0] {
        for j in 0..segments {
            let (a0, a1) = (j as f32 / segments as f32 * TAU, (j + 1) as f32 / segments as f32 * TAU);
            // Punto del círculo de la aurora a la altura h (sube hacia afuera, en la normal).
            let p = |a: f32, h: f32| {
                let dir = Vec3::new(a.cos() * AURORA_COLATITUDE.sin(), pole * AURORA_COLATITUDE.cos(), a.sin() * AURORA_COLATITUDE.sin());
                dir * (1.05 + h * AURORA_HEIGHT)
            };
            add_quad_both_sides(&mut m, [(p(a0, 0.0), (a0, 0.0)), (p(a1, 0.0), (a1, 0.0)), (p(a1, 1.0), (a1, 1.0)), (p(a0, 1.0), (a0, 1.0))]);
        }
    }
    m.recompute_normals();
    m.compute_bounds();
    m
}

/// Transición suave de 0 a 1 entre a y b.
fn smoothstep(a: f32, b: f32, x: f32) -> f32 {
    let t = ((x - a) / (b - a)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Pulso de 0.6 a 1 con el tiempo.
fn pulse(time: f32, speed: f32) -> f32 {
    0.8 + 0.2 * (time * speed).sin()
}

/// Batería y torres: color por banda de la paleta. El metal tiene Lambert + un borde verde;
/// la banda de brillo es EMISIVA y pulsa (se ve también de noche); el adorno brilla un poco.
struct OaPropShader {
    palette: [Vec3; 3],
}

impl FragmentShader for OaPropShader {
    fn shade(&self, f: &Fragment, u: &Uniforms) -> Vec3 {
        let band = ((f.uv.0 * 3.0).floor() as usize).min(2);
        let base = self.palette[band];
        let l = (u.light_pos - f.world_pos).normalized();
        let v = (u.camera_pos - f.world_pos).normalized();
        let diff = f.normal.dot(l).max(0.0);
        let rim = (1.0 - f.normal.dot(v).max(0.0)).powf(3.0);
        let lit = base * (0.15 + 0.85 * diff) + OA_GREEN * (rim * 0.35);
        match band {
            1 => lit + OA_GREEN * (0.7 * pulse(u.time, BATTERY_PULSE_SPEED)),
            2 => lit + base * 0.6, // puntas y aros: brillan un poco (se ven de lejos)
            _ => lit,
        }
    }
}

/// Aura de la batería: brilla al centro y se desvanece en la orilla; pulsa con el núcleo.
struct BatteryAuraShader;

impl FragmentShader for BatteryAuraShader {
    fn shade(&self, f: &Fragment, u: &Uniforms) -> Vec3 {
        let v = (u.camera_pos - f.world_pos).normalized();
        let facing = f.normal.dot(v).abs();
        OA_GREEN * (facing.powf(3.0) * 0.35 * pulse(u.time, BATTERY_PULSE_SPEED))
    }
}

/// Anillo de energía: bandas concéntricas (como los anillos de Saturno, con huecos de ruido)
/// y franjas brillantes que avanzan alrededor del planeta con el tiempo.
struct RingShader;

impl FragmentShader for RingShader {
    fn shade(&self, f: &Fragment, u: &Uniforms) -> Vec3 {
        let p = f.local_pos;
        let r = (p.x * p.x + p.z * p.z).sqrt();
        let s = (r - RING_INNER) / (RING_OUTER - RING_INNER); // 0 adentro, 1 afuera
        let angle = p.z.atan2(p.x);
        // Bandas: ruido a lo largo del radio → zonas más densas y huecos.
        let bands = (fbm(Vec3::new(r * 6.0, 0.0, 0.0), 3, 7) * 0.5 + 0.5).powf(1.5) * 1.6;
        // Franjas que fluyen: diente de sierra en el ángulo que avanza con el tiempo; cada
        // anillo interior va más rápido (como una órbita de verdad).
        let speed = RING_FLOW_SPEED / r.max(0.1);
        let k = angle / TAU * RING_STREAKS + u.time * speed + fbm(Vec3::new(r * 1.5, 1.0, 0.0), 2, 9) * 0.4;
        let streak = (k - k.floor()).powf(12.0);
        // Orillas suaves para que no se vea un corte duro.
        let edges = smoothstep(0.0, 0.12, s) * (1.0 - smoothstep(0.85, 1.0, s));
        let color = OA_GREEN * (0.35 * bands) + Vec3::new(0.6, 1.0, 0.7) * (streak * 0.8);
        color * (edges * RING_BRIGHTNESS)
    }
}

/// Aurora: cortinas verdes → turquesa que ondulan (dos senos que se mueven a distinta
/// velocidad), brillantes abajo y que se apagan hacia arriba.
struct AuroraShader;

impl FragmentShader for AuroraShader {
    fn shade(&self, f: &Fragment, u: &Uniforms) -> Vec3 {
        let (a, h) = f.uv;
        let t = u.time * AURORA_SPEED;
        let wave = (a * 7.0 + t + 2.0 * (a * 3.0 - t * 0.6).sin()).sin() * 0.5 + 0.5;
        let folds = (a * 23.0 - t * 1.7).sin() * 0.5 + 0.5;
        let curtain = wave * wave * (0.6 + 0.4 * folds);
        let fade = smoothstep(0.0, 0.12, h) * (1.0 - h).powf(1.5);
        let color = OA_GREEN * (1.0 - h) + OA_TEAL * h;
        color * (curtain * fade * AURORA_BRIGHTNESS)
    }
}
