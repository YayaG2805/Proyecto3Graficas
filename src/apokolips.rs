//! APOKOLIPS, el mundo de Darkseid. Es el premio de los cristales: aparece al juntar los 10
//! (o con la tecla K, para la demo).
//!
//!   1. BOOM TUBE: un portal de luz que se abre de frente a la cámara (anillo ADITIVO con una
//!      espiral que gira), el planeta crece desde su centro y el portal se cierra.
//!   2. PLANETA: corteza de fábricas oscuras partida en celdas (ruido Voronoi). Algunas celdas
//!      son FOSOS DE FUEGO hundidos que brillan y titilan, y por algunas orillas entre celdas
//!      corren grietas de lava que laten (shader animado). La malla se deforma con fBm y se
//!      hunde en los fosos (la misma función decide la forma y el color).
//!   3. DARKSEID: gigante de pie sobre el planeta, con los ojos rojos encendidos (EMISIVOS).
//!   4. NAVE DE GUERRA: da vueltas alrededor del planeta con ventanas y motores encendidos.
//!
//! El planeta se arma al cargar el juego pero entra a la escena (al final de `bodies`, para
//! no mover los índices de los demás) recién al abrirse el portal. Desde ahí es un mundo más:
//! tecla 6, warp, órbita, escaneo, gravedad y colisión funcionan igual que en los otros.

use crate::math::{Mat4, Vec3};
use crate::mesh::Mesh;
use crate::mundos::{on_surface, oriented, quad_both_sides, smoothstep, PaletteShader};
use crate::noise::{fbm, hash01, voronoi3, Cell};
use crate::renderer::{Fragment, FragmentShader, Renderer, Uniforms};
use crate::scene::{Body, PlanetInfo, Scene};
use crate::shaders::lambert;
use std::sync::Arc;

pub const NAME: &str = "Apokolips";

// --- el planeta ---
const RADIUS: f32 = 5.0;
/// Más lejos que Oa (160): es el mundo de afuera.
const ORBIT_RADIUS: f32 = 195.0;
const ORBIT_SPEED: f32 = 0.016;
const SPIN_SPEED: f32 = 0.15;
const TILT: f32 = 0.35;
const GRAVITY: f32 = 120.0;
/// Semilla de la corteza, los fosos y las grietas.
const SEED: u32 = 88;
/// Tamaño de las celdas de la corteza (más alto = celdas más chicas).
const CELL_FREQ: f32 = 5.0;
/// Fracción de celdas que son fosos de fuego.
const PIT_CHANCE: f32 = 0.18;
/// Relieve de la corteza y hundimiento de los fosos, en fracción del radio
/// (la colisión de la nave aguanta hasta 12 %).
const RELIEF: f32 = 0.035;
const PIT_DEPTH: f32 = 0.025;
/// Qué tanto de las orillas entre celdas son grietas de lava (0 a 1).
const CRACK_AMOUNT: f32 = 0.45;
const CRACK_WIDTH: f32 = 0.06;
const CRACK_PULSE_SPEED: f32 = 2.0;
const PIT_FLICKER_SPEED: f32 = 6.0;
const CRUST_DARK: Vec3 = Vec3::new(0.10, 0.09, 0.09);
const CRUST_LIGHT: Vec3 = Vec3::new(0.30, 0.26, 0.24);
const LAVA: Vec3 = Vec3::new(1.0, 0.35, 0.05);
const FIRE: Vec3 = Vec3::new(1.0, 0.6, 0.15);
/// Al aparecer, el planeta queda un poco ADELANTE de la nave en su vuelta al Sol (radianes),
/// así sale "del lado" donde está el jugador.
const AHEAD: f32 = 0.12;

// --- el Boom Tube (segundos desde que se abre) ---
const PORTAL_OPEN: f32 = 1.0;
const GROW_START: f32 = 1.0;
const GROW_END: f32 = 4.0;
pub const PORTAL_CLOSE: f32 = 5.0;
const PORTAL_RADIUS: f32 = 10.0;
const PORTAL_SPIRALS: f32 = 3.0;
const PORTAL_SPIN: f32 = 5.0;
const PORTAL_CORE: Vec3 = Vec3::new(1.0, 1.0, 1.0);
const PORTAL_MID: Vec3 = Vec3::new(0.35, 0.65, 1.0);
const PORTAL_EDGE: Vec3 = Vec3::new(1.0, 0.55, 0.2);
/// Segundos que dura el letrero de "se abrió un Boom Tube".
pub const MESSAGE_TIME: f32 = 7.0;

// --- Darkseid ---
/// Dónde está parado (dirección en el marco del planeta) y su tamaño: un coloso que se ve
/// desde la órbita.
const DARKSEID_DIR: Vec3 = Vec3::new(0.35, 0.9, 0.25);
const DARKSEID_SIZE: f32 = 1.4;
/// Paleta en el MISMO orden que en assets/props/modelar_apokolips.py: piedra, armadura, oscuro, ojos.
const DARKSEID_PALETTE: [Vec3; 4] =
    [Vec3::new(0.45, 0.43, 0.41), Vec3::new(0.13, 0.17, 0.38), Vec3::new(0.07, 0.07, 0.09), Vec3::new(1.0, 0.12, 0.05)];

// --- su nave de guerra ---
const SHIP_ORBIT_RADII: f32 = 2.0;
const SHIP_SPEED: f32 = 0.35;
const SHIP_TILT: f32 = 0.6;
const SHIP_SIZE: f32 = 1.6;
/// Paleta: casco, detalles, ventanas, motores (mismo orden que en el script de Blender).
const SHIP_PALETTE: [Vec3; 4] =
    [Vec3::new(0.24, 0.22, 0.24), Vec3::new(0.11, 0.10, 0.11), Vec3::new(1.0, 0.22, 0.05), Vec3::new(1.0, 0.55, 0.12)];

fn is_pit(cell: Cell) -> bool {
    hash01(cell.0, cell.1, cell.2, SEED.wrapping_add(7)) < PIT_CHANCE
}

/// Qué tan dentro de un foso de fuego está la dirección `d`: 1 al centro, 0 fuera.
fn pit_amount(d: Vec3) -> f32 {
    let (f1, _, cell) = voronoi3(d.normalized() * CELL_FREQ, SEED);
    if is_pit(cell) { 1.0 - smoothstep(0.1, 0.45, f1) } else { 0.0 }
}

/// Forma de la superficie: corteza irregular (fBm) con los fosos hundidos.
fn apokolips_surface(d: Vec3) -> f32 {
    1.0 + (fbm(d * 4.0, 3, SEED) * 0.5 + 0.5) * RELIEF - pit_amount(d) * PIT_DEPTH
}

/// Carga un modelo con sus LODs (todos de radio 1). Si falta, usa una esfera.
fn load_lods(base: &str) -> Vec<Mesh> {
    let lods: Vec<Mesh> =
        ["", "_lod1", "_lod2"].iter().filter_map(|s| Mesh::load_obj(&format!("{base}{s}.obj"), Some(1.0)).ok()).collect();
    if lods.is_empty() {
        eprintln!("Falta {base}.obj: se usa una esfera");
        return vec![Mesh::icosphere(1, &|_| 1.0)];
    }
    lods
}

pub struct Apokolips {
    /// El planeta ya armado, esperando a que se abra el portal.
    body: Option<Body>,
    /// Su lugar en `scene.bodies` cuando ya apareció.
    index: Option<usize>,
    /// Segundos desde que se abrió el portal.
    age: f32,
    darkseid: Vec<Mesh>,
    /// Qué tan abajo del centro del modelo están los pies (en múltiplos de su tamaño).
    darkseid_feet: f32,
    darkseid_shader: Arc<dyn FragmentShader>,
    ship: Vec<Mesh>,
    ship_shader: Arc<dyn FragmentShader>,
    portal: Mesh,
}

impl Apokolips {
    pub fn new() -> Apokolips {
        let body = Body {
            name: NAME,
            parent: None,
            gravity: GRAVITY,
            info: PlanetInfo {
                title: "Apokolips",
                hero: "Darkseid (villano)",
                fact: "Planeta de fuego y fábricas gobernado por Darkseid, que busca la Ecuación de la Antivida.",
            },
            props: Vec::new(),
            radius: RADIUS,
            orbit_radius: ORBIT_RADIUS,
            orbit_speed: ORBIT_SPEED,
            orbit_phase: 0.0,
            spin_speed: SPIN_SPEED,
            tilt: TILT,
            surface: apokolips_surface,
            lods: [5, 4, 3, 2].iter().map(|&s| Mesh::icosphere(s, &apokolips_surface)).collect(),
            shader: Arc::new(ApokolipsShader),
        };
        let darkseid = load_lods("assets/heroes/darkseid");
        let darkseid_feet = -darkseid[0].vertices.iter().map(|v| v.position.y).fold(f32::MAX, f32::min);

        // El portal: un disco plano en XZ (normal +Y). uv.0 = ángulo (0 a 1), uv.1 = radio (0 a 1).
        let mut portal = Mesh::default();
        let segments = 48;
        for j in 0..segments {
            let (a0, a1) = (j as f32 / segments as f32, (j + 1) as f32 / segments as f32);
            let point = |a: f32, r: f32| {
                let ang = a * std::f32::consts::TAU;
                Vec3::new(ang.cos() * r, 0.0, ang.sin() * r)
            };
            quad_both_sides(
                &mut portal,
                [(point(a0, 0.02), (a0, 0.0)), (point(a1, 0.02), (a1, 0.0)), (point(a1, 1.0), (a1, 1.0)), (point(a0, 1.0), (a0, 1.0))],
            );
        }
        portal.compute_bounds();

        Apokolips {
            body: Some(body),
            index: None,
            age: 0.0,
            darkseid,
            darkseid_feet,
            // los ojos (banda 3) brillan solos: los Rayos Omega
            darkseid_shader: Arc::new(PaletteShader {
                palette: DARKSEID_PALETTE.to_vec(),
                glow: vec![(3, 2.5)],
                rim: Vec3::new(0.8, 0.3, 0.2),
                blink: None,
            }),
            ship: load_lods("assets/props/nave_apokolips"),
            // ventanas (banda 2) y motores (banda 3) encendidos; las ventanas parpadean
            ship_shader: Arc::new(PaletteShader {
                palette: SHIP_PALETTE.to_vec(),
                glow: vec![(2, 1.5), (3, 2.0)],
                rim: Vec3::new(0.7, 0.25, 0.15),
                blink: Some(2),
            }),
            portal,
        }
    }

    /// Índice en `scene.bodies`, si ya apareció.
    pub fn index(&self) -> Option<usize> {
        self.index
    }

    /// Segundos desde que se abrió el portal (None si todavía no se abre).
    pub fn age(&self) -> Option<f32> {
        self.index.map(|_| self.age)
    }

    /// Abre el portal: el planeta entra a la escena un poco adelante de la nave en su vuelta
    /// al Sol, todavía diminuto. Devuelve false si ya había aparecido.
    pub fn unlock(&mut self, scene: &mut Scene, ship_pos: Vec3, t: f32) -> bool {
        let Some(mut body) = self.body.take() else { return false };
        // ángulo de la nave alrededor del Sol; el planeta sale ahí (+AHEAD) en su órbita
        let angle = ship_pos.z.atan2(ship_pos.x) + AHEAD;
        body.orbit_phase = angle - ORBIT_SPEED * t;
        body.radius = RADIUS * 0.002;
        scene.bodies.push(body);
        self.index = Some(scene.bodies.len() - 1);
        self.age = 0.0;
        true
    }

    /// Avanza la aparición: el planeta crece de 0 a su radio mientras el portal está abierto.
    /// `dt` = 0 en modo foto.
    pub fn update(&mut self, scene: &mut Scene, dt: f32) {
        let Some(i) = self.index else { return };
        self.age += dt;
        scene.bodies[i].radius = RADIUS * self.grown().max(0.002);
    }

    /// Para las capturas: salta al final de la aparición.
    pub fn finish(&mut self, scene: &mut Scene) {
        self.update(scene, PORTAL_CLOSE + 1.0);
    }

    /// De 0 (recién abierto el portal) a 1 (el planeta ya llegó a su tamaño).
    fn grown(&self) -> f32 {
        smoothstep(GROW_START, GROW_END, self.age)
    }

    /// Dónde están los pies de Darkseid y su "arriba" (para las capturas).
    pub fn darkseid_spot(&self, scene: &Scene, t: f32) -> Option<(Vec3, Vec3)> {
        let i = self.index?;
        let frame = on_surface(scene, i, DARKSEID_DIR, 0.0, t);
        Some((frame.transform_point(Vec3::ZERO), frame.transform_dir(Vec3::UP).normalized()))
    }

    fn darkseid_model(&self, scene: &Scene, i: usize, t: f32) -> Mat4 {
        let size = DARKSEID_SIZE * self.grown();
        on_surface(scene, i, DARKSEID_DIR, 0.8, t)
            * Mat4::translation(Vec3::new(0.0, size * self.darkseid_feet, 0.0))
            * Mat4::scale(Vec3::splat(size.max(0.001)))
    }

    /// Matriz de la nave: vuela en círculo alrededor del planeta en un plano inclinado.
    pub fn ship_model(&self, scene: &Scene, t: f32) -> Option<Mat4> {
        let i = self.index?;
        let center = scene.body_position(i, t);
        let r = scene.bodies[i].radius * SHIP_ORBIT_RADII;
        let a = t * SHIP_SPEED;
        let e1 = Vec3::new(1.0, 0.0, 0.0);
        let e2 = Vec3::new(0.0, SHIP_TILT.sin(), SHIP_TILT.cos());
        let pos = center + (e1 * a.cos() + e2 * a.sin()) * r;
        let forward = e1 * -a.sin() + e2 * a.cos();
        let size = SHIP_SIZE * self.grown();
        Some(oriented(pos, e1.cross(e2) * -1.0, forward) * Mat4::scale(Vec3::splat(size.max(0.001))))
    }

    /// Lo opaco: Darkseid y su nave (el planeta lo dibuja la escena, como los demás).
    pub fn draw(&self, r: &mut Renderer, scene: &Scene, t: f32) {
        let Some(i) = self.index else { return };
        r.draw_lods(&self.darkseid, &self.darkseid_model(scene, i, t), self.darkseid_shader.clone());
        if let Some(m) = self.ship_model(scene, t) {
            r.draw_lods(&self.ship, &m, self.ship_shader.clone());
        }
    }

    /// Lo aditivo: el Boom Tube mientras está abierto, siempre de frente a la cámara.
    pub fn draw_glow(&self, r: &mut Renderer, scene: &Scene, t: f32) {
        let Some(i) = self.index else { return };
        let open = smoothstep(0.0, PORTAL_OPEN, self.age) * (1.0 - smoothstep(GROW_END, PORTAL_CLOSE, self.age));
        if open <= 0.0 {
            return;
        }
        let center = scene.body_position(i, t);
        let to_camera = (r.uniforms.camera_pos - center).normalized();
        let side = if to_camera.y.abs() < 0.9 { Vec3::UP } else { Vec3::new(1.0, 0.0, 0.0) };
        let size = PORTAL_RADIUS * (0.3 + 0.7 * open);
        let model = oriented(center, to_camera, side.cross(to_camera)) * Mat4::scale(Vec3::splat(size));
        r.draw_mesh_additive(&self.portal, &model, Arc::new(BoomTubeShader { light: open }));
    }
}

/// Corteza de fábricas, fosos de fuego que titilan y grietas de lava que laten.
struct ApokolipsShader;

impl FragmentShader for ApokolipsShader {
    fn shade(&self, f: &Fragment, u: &Uniforms) -> Vec3 {
        // paso 1: la celda de Voronoi y qué tan cerca de su orilla está el punto
        let p = f.local_pos.normalized();
        let (f1, f2, cell) = voronoi3(p * CELL_FREQ, SEED);
        let border = f2 - f1;

        // paso 2: corteza con tonos de ruido y una ranura oscura en cada orilla (las "calles")
        let tone = fbm(p * 9.0, 3, SEED.wrapping_add(1)) * 0.5 + 0.5;
        let groove = 1.0 - smoothstep(0.0, 0.035, border);
        let crust = CRUST_DARK.lerp(CRUST_LIGHT, tone) * (1.0 - 0.6 * groove);
        let lit = crust * lambert(f, u, 0.08);

        // paso 3: grietas de lava en parte de las orillas, con un pulso que recorre el planeta
        let cracks = smoothstep(1.0 - CRACK_AMOUNT, 1.0, fbm(p * 3.0, 2, SEED.wrapping_add(2)) * 0.5 + 0.5);
        let pulse = 0.6 + 0.4 * (u.time * CRACK_PULSE_SPEED + fbm(p * 4.0, 2, SEED) * 6.0).sin();
        let lava = (1.0 - smoothstep(0.0, CRACK_WIDTH, border)) * cracks * pulse;

        // paso 4: los fosos de fuego brillan más al centro y titilan cada uno a su ritmo
        let fire = if is_pit(cell) {
            let flicker = 0.7 + 0.3 * (u.time * PIT_FLICKER_SPEED + hash01(cell.0, cell.1, cell.2, SEED) * 20.0).sin();
            (1.0 - smoothstep(0.1, 0.45, f1)) * flicker
        } else {
            0.0
        };

        // lo que brilla (lava y fuego) es EMISIVO: se ve también del lado de noche
        lit + LAVA * (lava * 1.4) + FIRE * (fire * 1.6)
    }
}

/// El Boom Tube: centro blanco, una espiral azul que gira y una orilla naranja.
struct BoomTubeShader {
    light: f32,
}

impl FragmentShader for BoomTubeShader {
    fn shade(&self, f: &Fragment, u: &Uniforms) -> Vec3 {
        let (ang, rad) = (f.uv.0 * std::f32::consts::TAU, f.uv.1.clamp(0.0, 1.0));
        let swirl = (ang * PORTAL_SPIRALS + rad * 10.0 - u.time * PORTAL_SPIN).sin() * 0.5 + 0.5;
        let core = 1.0 - smoothstep(0.0, 0.5, rad);
        let edge = smoothstep(0.75, 0.93, rad) * (1.0 - smoothstep(0.93, 1.0, rad));
        (PORTAL_CORE * (core * 1.5) + PORTAL_MID * (swirl * (1.0 - rad) * 0.9) + PORTAL_EDGE * (edge * 1.2)) * self.light
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ship::collision_radius;

    #[test]
    fn la_superficie_no_pasa_de_la_colision() {
        // la colisión de la nave supone que nada sube más de 12 % del radio
        for k in 0..2000 {
            let d = Vec3::new(hash01(k, 1, 0, 3) - 0.5, hash01(k, 2, 0, 3) - 0.5, hash01(k, 3, 0, 3) - 0.5);
            let s = apokolips_surface(d.normalized());
            assert!(s > 0.9 && s < 1.12, "superficie {s}");
        }
        assert!(collision_radius(RADIUS) > RADIUS * 1.12);
    }

    #[test]
    fn hay_fosos_de_fuego_pero_no_es_todo_fuego() {
        let pits = (0..2000)
            .filter(|&k| {
                let d = Vec3::new(hash01(k, 1, 0, 9) - 0.5, hash01(k, 2, 0, 9) - 0.5, hash01(k, 3, 0, 9) - 0.5);
                pit_amount(d.normalized()) > 0.5
            })
            .count();
        assert!(pits > 20 && pits < 600, "fosos: {pits} de 2000");
    }

    #[test]
    fn el_portal_hace_crecer_el_planeta_y_solo_aparece_una_vez() {
        let mut scene = Scene::new();
        let before = scene.bodies.len();
        let mut a = Apokolips::new();
        assert!(a.unlock(&mut scene, Vec3::new(0.0, 0.0, 70.0), 0.0));
        assert!(!a.unlock(&mut scene, Vec3::new(0.0, 0.0, 70.0), 0.0), "no se agrega dos veces");
        assert_eq!(scene.bodies.len(), before + 1);
        let i = a.index().unwrap();
        assert!(scene.bodies[i].radius < 0.1);
        a.update(&mut scene, 2.5);
        let half = scene.bodies[i].radius;
        assert!(half > 0.5 && half < RADIUS);
        a.update(&mut scene, 3.0);
        assert_eq!(scene.bodies[i].radius, RADIUS);
        // aparece del lado de la nave (la nave estaba en +z)
        assert!(scene.body_position(i, 0.0).z > ORBIT_RADIUS * 0.9);
    }

    #[test]
    fn darkseid_y_su_nave_tienen_modelo() {
        assert!(Mesh::load_obj("assets/heroes/darkseid.obj", Some(1.0)).is_ok());
        assert!(Mesh::load_obj("assets/props/nave_apokolips.obj", Some(1.0)).is_ok());
        let a = Apokolips::new();
        assert_eq!(a.darkseid.len(), 3);
        assert_eq!(a.ship.len(), 3);
        assert!(a.darkseid_feet > 0.5, "los pies quedan abajo del centro");
    }
}
