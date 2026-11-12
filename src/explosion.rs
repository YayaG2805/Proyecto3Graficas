//! EXPLOSIÓN de la nave al chocar con el Sol, y dónde reaparece después.
//!
//!   1. PEDAZOS: al cargar la nave, sus triángulos se agrupan en `PIECE_COUNT` pedazos
//!      (generación de geometría antes de rasterizar). Se eligen centros lo más separados
//!      posible (muestreo del punto más lejano) y cada triángulo va con el centro más cercano;
//!      luego los centros se mueven al promedio de su grupo (como k-means, PIECE_ITERATIONS veces).
//!      Al explotar, cada pedazo sale disparado desde el centro de la nave, girando sobre su
//!      propio eje, al rojo vivo y enfriándose. Al final se encogen hasta desaparecer.
//!   2. BOLA DE FUEGO: dos esferas ADITIVAS (núcleo blanco y fuego naranja) que crecen y se
//!      apagan; el fuego se mueve con ruido fBm.
//!   3. ONDA EXPANSIVA: un anillo plano de luz que se abre sobre la superficie del Sol.
//!
//! Todo depende de `age` (segundos desde el choque), así que en modo foto se congela.

use crate::math::{Mat4, Vec3};
use crate::mesh::{Mesh, Vertex};
use crate::mundos::{oriented, quad_both_sides, smoothstep};
use crate::noise::{fbm, hash01};
use crate::renderer::{Fragment, FragmentShader, Renderer, Uniforms};
use crate::scene::Scene;
use crate::shaders::lambert;
use crate::ship::collision_radius;
use std::sync::Arc;

/// Segundos que dura la explosión antes de reaparecer.
pub const EXPLOSION_TIME: f32 = 3.0;
/// Segundos que dura el destello blanco de la pantalla.
pub const FLASH_TIME: f32 = 0.45;
/// Segundos (y fuerza, en unidades del mundo) de la sacudida de la cámara.
pub const SHAKE_TIME: f32 = 0.9;
pub const SHAKE_STRENGTH: f32 = 0.6;

// --- pedazos ---
const PIECE_COUNT: usize = 14;
const PIECE_ITERATIONS: usize = 3;
/// Semilla de las velocidades y giros de los pedazos: misma semilla = misma explosión.
const PIECE_SEED: u32 = 99;
/// Rapidez inicial de cada pedazo (unidades/s), entre estos dos valores.
const PIECE_SPEED_MIN: f32 = 5.0;
const PIECE_SPEED_MAX: f32 = 11.0;
/// Freno de los pedazos: recorren como máximo rapidez / freno.
const PIECE_DRAG: f32 = 1.2;
/// Giro de cada pedazo (rad/s), entre estos dos valores.
const PIECE_SPIN_MIN: f32 = 1.5;
const PIECE_SPIN_MAX: f32 = 6.0;
/// Cuánto se empujan los pedazos hacia afuera del Sol (si no, la mitad se metería en él).
const PIECE_AWAY: f32 = 0.8;
/// Segundos en que los pedazos pasan de amarillo a metal frío.
const PIECE_COOL_TIME: f32 = 2.2;
/// Desde qué segundo los pedazos se encogen hasta desaparecer al final.
const PIECE_SHRINK_START: f32 = 2.4;
const PIECE_HOT: Vec3 = Vec3::new(1.0, 0.75, 0.25);
const PIECE_WARM: Vec3 = Vec3::new(0.9, 0.18, 0.03);
const PIECE_METAL: Vec3 = Vec3::new(0.35, 0.34, 0.33);

// --- bola de fuego ---
const CORE_RADIUS: f32 = 2.5;
const FIRE_RADIUS: f32 = 5.0;
const FIRE_SEED: u32 = 123;
const CORE_INNER: Vec3 = Vec3::new(1.0, 1.0, 0.9);
const CORE_OUTER: Vec3 = Vec3::new(1.0, 0.8, 0.4);
const FIRE_INNER: Vec3 = Vec3::new(1.0, 0.65, 0.15);
const FIRE_OUTER: Vec3 = Vec3::new(0.7, 0.12, 0.02);

// --- onda expansiva ---
const RING_RADIUS: f32 = 14.0;
const RING_TIME: f32 = 1.4;
/// Grosor del anillo como fracción del radio (0.3 = el 30% de afuera).
const RING_WIDTH: f32 = 0.3;
const RING_SEGMENTS: usize = 48;
const RING_COLOR: Vec3 = Vec3::new(1.0, 0.75, 0.45);

// --- reaparición ---
/// La nave reaparece a esta altura sobre la superficie del mundo elegido, en radios del mundo
/// (dentro del rango de la órbita, así se puede apretar F en cuanto aparece).
pub const RESPAWN_ALTITUDE_RADII: f32 = 2.0;

/// Un pedazo de la nave: su malla (centrada en su propio centro) y cómo sale volando.
struct Piece {
    mesh: Mesh,
    /// Centro del pedazo en el espacio de la nave.
    center: Vec3,
    speed: f32,
    axis: Vec3,
    spin: f32,
}

/// Lo que se prepara una sola vez al cargar la nave.
pub struct ExplosionAssets {
    pieces: Vec<Piece>,
    fireball: Mesh,
    ring: Mesh,
}

/// Una dirección al azar (uniforme sobre la esfera) a partir de dos números de 0 a 1.
fn random_dir(a: f32, b: f32) -> Vec3 {
    let y = a * 2.0 - 1.0;
    let ang = b * std::f32::consts::TAU;
    let r = (1.0 - y * y).max(0.0).sqrt();
    Vec3::new(r * ang.cos(), y, r * ang.sin())
}

/// Rotación de `angle` radianes alrededor del eje `axis` (fórmula de Rodrigues):
/// R = cos·I + (1 − cos)·k·kᵀ + sen·[k]ₓ
fn rotation_axis(axis: Vec3, angle: f32) -> Mat4 {
    let k = axis.normalized();
    let (s, c) = angle.sin_cos();
    let t = 1.0 - c;
    let mut m = Mat4::identity();
    m.m[0] = [c + t * k.x * k.x, t * k.x * k.y - s * k.z, t * k.x * k.z + s * k.y, 0.0];
    m.m[1] = [t * k.y * k.x + s * k.z, c + t * k.y * k.y, t * k.y * k.z - s * k.x, 0.0];
    m.m[2] = [t * k.z * k.x - s * k.y, t * k.z * k.y + s * k.x, c + t * k.z * k.z, 0.0];
    m
}

/// Parte la malla en `count` grupos de triángulos cercanos. Devuelve el grupo de cada triángulo.
fn cluster_triangles(centroids: &[Vec3], count: usize) -> Vec<usize> {
    if centroids.is_empty() {
        return Vec::new();
    }
    // paso 1: centros iniciales lo más separados posible. El primero es el triángulo más lejano
    // del centro de la nave; cada siguiente, el más lejano de todos los que ya hay.
    let farthest = |from: &dyn Fn(Vec3) -> f32| {
        (0..centroids.len()).max_by(|&a, &b| from(centroids[a]).total_cmp(&from(centroids[b]))).unwrap_or(0)
    };
    let mut seeds = vec![centroids[farthest(&|p| p.length())]];
    while seeds.len() < count.min(centroids.len()) {
        let next = farthest(&|p| seeds.iter().map(|s| (p - *s).length()).fold(f32::MAX, f32::min));
        seeds.push(centroids[next]);
    }
    // paso 2: cada triángulo al centro más cercano, y cada centro al promedio de su grupo
    let nearest = |p: Vec3, seeds: &[Vec3]| {
        (0..seeds.len()).min_by(|&a, &b| (p - seeds[a]).length().total_cmp(&(p - seeds[b]).length())).unwrap_or(0)
    };
    let mut group: Vec<usize> = centroids.iter().map(|&p| nearest(p, &seeds)).collect();
    for _ in 0..PIECE_ITERATIONS {
        let mut sum = vec![(Vec3::ZERO, 0usize); seeds.len()];
        for (p, &g) in centroids.iter().zip(&group) {
            sum[g].0 += *p;
            sum[g].1 += 1;
        }
        for (s, (total, n)) in seeds.iter_mut().zip(sum) {
            if n > 0 {
                *s = total / n as f32;
            }
        }
        group = centroids.iter().map(|&p| nearest(p, &seeds)).collect();
    }
    group
}

impl ExplosionAssets {
    pub fn new(ship: &Mesh) -> ExplosionAssets {
        // paso 1: el centro de cada triángulo y a qué pedazo le toca
        let tris: Vec<[Vertex; 3]> = ship
            .indices
            .chunks_exact(3)
            .map(|t| [ship.vertices[t[0] as usize], ship.vertices[t[1] as usize], ship.vertices[t[2] as usize]])
            .collect();
        let centroids: Vec<Vec3> = tris.iter().map(|t| (t[0].position + t[1].position + t[2].position) / 3.0).collect();
        let group = cluster_triangles(&centroids, PIECE_COUNT);

        // paso 2: una malla por pedazo, con sus vértices medidos desde el centro del pedazo
        // (así gira sobre sí mismo y no alrededor de la nave)
        let mut pieces = Vec::new();
        for g in 0..PIECE_COUNT {
            let mine: Vec<&[Vertex; 3]> = tris.iter().zip(&group).filter(|(_, &k)| k == g).map(|(t, _)| t).collect();
            if mine.is_empty() {
                continue;
            }
            let center = mine.iter().map(|t| (t[0].position + t[1].position + t[2].position) / 3.0).fold(Vec3::ZERO, |a, p| a + p)
                / mine.len() as f32;
            let mut mesh = Mesh::default();
            for t in &mine {
                let base = mesh.vertices.len() as u32;
                for v in t.iter() {
                    mesh.vertices.push(Vertex { position: v.position - center, ..*v });
                }
                mesh.indices.extend([base, base + 1, base + 2]);
            }
            mesh.compute_bounds();
            // paso 3: rapidez, eje y giro propios, con hash: misma semilla = misma explosión
            let k = pieces.len() as i32;
            let h = |n| hash01(k, n, 0, PIECE_SEED);
            pieces.push(Piece {
                mesh,
                center,
                speed: PIECE_SPEED_MIN + (PIECE_SPEED_MAX - PIECE_SPEED_MIN) * h(1),
                axis: random_dir(h(2), h(3)),
                spin: PIECE_SPIN_MIN + (PIECE_SPIN_MAX - PIECE_SPIN_MIN) * h(4),
            });
        }

        // La bola de fuego es una icosfera; la onda, un anillo plano en XZ (normal +Y).
        // En el anillo uv.1 va de 0 (orilla de adentro) a 1 (orilla de afuera).
        let mut ring = Mesh::default();
        for j in 0..RING_SEGMENTS {
            let (a0, a1) = (j as f32 / RING_SEGMENTS as f32, (j + 1) as f32 / RING_SEGMENTS as f32);
            let point = |a: f32, r: f32| {
                let ang = a * std::f32::consts::TAU;
                Vec3::new(ang.cos() * r, 0.0, ang.sin() * r)
            };
            let inner = 1.0 - RING_WIDTH;
            quad_both_sides(
                &mut ring,
                [(point(a0, inner), (a0, 0.0)), (point(a1, inner), (a1, 0.0)), (point(a1, 1.0), (a1, 1.0)), (point(a0, 1.0), (a0, 1.0))],
            );
        }
        ring.compute_bounds();

        ExplosionAssets { pieces, fireball: Mesh::icosphere(3, &|_| 1.0), ring }
    }

    /// Cuántos triángulos suman todos los pedazos (deben ser los mismos que los de la nave).
    #[cfg(test)]
    fn piece_triangles(&self) -> usize {
        self.pieces.iter().map(|p| p.mesh.triangle_count()).sum()
    }
}

/// Una explosión en curso.
pub struct Explosion {
    /// Segundos desde el choque.
    pub age: f32,
    /// Dónde estaba la nave.
    pub center: Vec3,
    /// Hacia afuera del Sol, en el punto del choque.
    pub away: Vec3,
    /// Matriz de modelo de la nave en el momento del choque.
    model: Mat4,
}

impl Explosion {
    pub fn new(ship_model: Mat4, away: Vec3) -> Explosion {
        Explosion { age: 0.0, center: ship_model.transform_point(Vec3::ZERO), away: away.normalized(), model: ship_model }
    }

    /// Matriz de cada pedazo en este momento:
    ///   Model = T(centro + recorrido) · escala · giro propio · orientación de la nave al chocar
    fn piece_models(&self, assets: &ExplosionAssets) -> Vec<Mat4> {
        // la orientación de la nave es su matriz de modelo sin la traslación
        let orientation = Mat4::translation(-self.center) * self.model;
        let travel = (1.0 - (-PIECE_DRAG * self.age).exp()) / PIECE_DRAG; // se va frenando
        let size = 1.0 - smoothstep(PIECE_SHRINK_START, EXPLOSION_TIME, self.age);
        assets
            .pieces
            .iter()
            .map(|p| {
                let start = self.model.transform_point(p.center);
                let out = start - self.center;
                let out = if out.length() > 1e-4 { out.normalized() } else { self.away };
                let dir = (out + self.away * PIECE_AWAY).normalized();
                Mat4::translation(start + dir * (p.speed * travel))
                    * Mat4::scale(Vec3::splat(size.max(0.001)))
                    * rotation_axis(p.axis, p.spin * self.age)
                    * orientation
            })
            .collect()
    }

    /// Lo opaco: los pedazos de la nave.
    pub fn draw(&self, r: &mut Renderer, assets: &ExplosionAssets) {
        let heat = (1.0 - self.age / PIECE_COOL_TIME).max(0.0);
        let shader: Arc<dyn FragmentShader> = Arc::new(DebrisShader { heat: heat * heat });
        for (p, model) in assets.pieces.iter().zip(self.piece_models(assets)) {
            r.draw_mesh(&p.mesh, &model, shader.clone());
        }
    }

    /// Lo aditivo (luz): bola de fuego y onda expansiva. Va después de todo lo opaco.
    pub fn draw_glow(&self, r: &mut Renderer, assets: &ExplosionAssets) {
        let t = self.age;
        // el núcleo crece muy rápido y se apaga pronto; el fuego crece más lento y dura más
        let core = CORE_RADIUS * (1.0 - (-8.0 * t).exp());
        let core_light = 1.6 * (1.0 - smoothstep(0.0, 0.9, t));
        let fire = FIRE_RADIUS * (1.0 - (-4.0 * t).exp());
        let fire_light = 1.0 - smoothstep(0.3, 2.2, t);
        if fire_light > 0.0 {
            let m = Mat4::translation(self.center) * Mat4::scale(Vec3::splat(fire.max(0.01)));
            r.draw_mesh_additive(&assets.fireball, &m, Arc::new(FireShader { light: fire_light, inner: FIRE_INNER, outer: FIRE_OUTER }));
        }
        if core_light > 0.0 {
            let m = Mat4::translation(self.center) * Mat4::scale(Vec3::splat(core.max(0.01)));
            r.draw_mesh_additive(&assets.fireball, &m, Arc::new(FireShader { light: core_light, inner: CORE_INNER, outer: CORE_OUTER }));
        }

        // la onda se abre sobre el plano tangente al Sol (su normal es `away`)
        let ring_light = 1.0 - smoothstep(0.2, RING_TIME, t);
        if ring_light > 0.0 {
            let side = if self.away.y.abs() < 0.9 { Vec3::UP } else { Vec3::new(1.0, 0.0, 0.0) };
            let size = RING_RADIUS * smoothstep(0.0, RING_TIME, t).sqrt();
            let m = oriented(self.center, self.away, side.cross(self.away)) * Mat4::scale(Vec3::splat(size.max(0.01)));
            r.draw_mesh_additive(&assets.ring, &m, Arc::new(ShockwaveShader { light: ring_light }));
        }
    }
}

/// Dónde reaparece la nave: `RESPAWN_ALTITUDE_RADII` sobre el lado iluminado del mundo `i`
/// (un poco hacia arriba para verlo de lado). Devuelve (posición, centro del mundo, velocidad del
/// mundo); la nave sale con la velocidad del mundo para no quedarse atrás mientras él orbita.
pub fn respawn_point(scene: &Scene, i: usize, t: f32) -> (Vec3, Vec3, Vec3) {
    let center = scene.body_position(i, t);
    let to_sun = if center.length() > 1e-3 { -center.normalized() } else { Vec3::new(0.0, 0.0, 1.0) };
    let dir = (to_sun * 0.8 + Vec3::UP * 0.6).normalized();
    let pos = center + dir * (scene.bodies[i].radius * (1.0 + RESPAWN_ALTITUDE_RADII));
    let velocity = (scene.body_position(i, t + 0.01) - center) / 0.01;
    (pos, center, velocity)
}

/// Qué tan cerca está un punto de la colisión del cuerpo `i`: 0 a `range` o más, 1 tocándola.
pub fn heat_near(scene: &Scene, i: usize, t: f32, p: Vec3, range: f32) -> f32 {
    let d = (scene.body_position(i, t) - p).length() - collision_radius(scene.bodies[i].radius);
    1.0 - smoothstep(0.0, range, d)
}

/// Metal de la nave al rojo vivo: amarillo al principio, rojo después y al final metal frío.
struct DebrisShader {
    /// 1 = recién explotado, 0 = ya frío.
    heat: f32,
}

impl FragmentShader for DebrisShader {
    fn shade(&self, f: &Fragment, u: &Uniforms) -> Vec3 {
        let metal = PIECE_METAL * lambert(f, u, 0.15);
        let glow = PIECE_WARM.lerp(PIECE_HOT, self.heat) * (self.heat * 1.5);
        metal + glow
    }
}

/// Fuego ADITIVO: más brillante donde la esfera nos mira de frente (el centro) y con
/// manchas de ruido que suben con el tiempo.
struct FireShader {
    light: f32,
    inner: Vec3,
    outer: Vec3,
}

impl FragmentShader for FireShader {
    fn shade(&self, f: &Fragment, u: &Uniforms) -> Vec3 {
        let v = (u.camera_pos - f.world_pos).normalized();
        let facing = f.normal.dot(v).abs();
        let swirl = fbm(f.local_pos * 2.5 + Vec3::new(0.0, -u.time * 1.5, 0.0), 3, FIRE_SEED) * 0.5 + 0.5;
        let k = (facing * facing * (0.6 + 0.8 * swirl)).min(1.0);
        self.outer.lerp(self.inner, k) * (k * self.light)
    }
}

/// Anillo de luz ADITIVO: brilla más cerca de la orilla de afuera y se apaga suave hacia las dos
/// orillas (si no, el borde se ve cortado). sen(π·uv)² vale 0 en las orillas y 1 en medio; el
/// uv^0.6 corre ese máximo hacia afuera.
struct ShockwaveShader {
    light: f32,
}

impl FragmentShader for ShockwaveShader {
    fn shade(&self, f: &Fragment, _u: &Uniforms) -> Vec3 {
        let band = (std::f32::consts::PI * f.uv.1.clamp(0.0, 1.0).powf(0.6)).sin();
        RING_COLOR * (band * band * self.light)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn los_pedazos_son_toda_la_nave() {
        let ship = Mesh::load_obj("assets/Halcon_Milenario.obj", Some(1.5)).unwrap_or_else(|_| Mesh::placeholder_ship());
        let assets = ExplosionAssets::new(&ship);
        assert!(assets.pieces.len() > 1);
        assert!(assets.pieces.len() <= PIECE_COUNT);
        // no se pierde ni se repite ningún triángulo
        assert_eq!(assets.piece_triangles(), ship.triangle_count());
    }

    #[test]
    fn la_misma_semilla_da_la_misma_explosion() {
        let ship = Mesh::placeholder_ship();
        let (a, b) = (ExplosionAssets::new(&ship), ExplosionAssets::new(&ship));
        for (p, q) in a.pieces.iter().zip(&b.pieces) {
            assert_eq!(p.speed, q.speed);
            assert_eq!((p.center - q.center).length(), 0.0);
        }
    }

    #[test]
    fn los_pedazos_se_alejan_y_al_final_desaparecen() {
        let ship = Mesh::placeholder_ship();
        let assets = ExplosionAssets::new(&ship);
        let mut e = Explosion::new(Mat4::translation(Vec3::new(10.0, 0.0, 0.0)), Vec3::new(1.0, 0.0, 0.0));
        let spread = |e: &Explosion| {
            e.piece_models(&assets).iter().map(|m| (m.transform_point(Vec3::ZERO) - e.center).length()).fold(0.0, f32::max)
        };
        let first = spread(&e);
        e.age = 1.0;
        let later = spread(&e);
        assert!(later > first + 3.0, "de {first} a {later}");
        // al terminar los pedazos ya no miden casi nada
        e.age = EXPLOSION_TIME;
        let m = e.piece_models(&assets)[0];
        assert!(m.transform_dir(Vec3::new(1.0, 0.0, 0.0)).length() < 0.01);
    }

    #[test]
    fn la_rotacion_no_deforma() {
        let r = rotation_axis(Vec3::new(1.0, 2.0, 3.0), 0.7);
        let v = Vec3::new(0.3, -1.2, 2.0);
        assert!((r.transform_dir(v).length() - v.length()).abs() < 1e-4);
        // el eje no se mueve
        let axis = Vec3::new(1.0, 2.0, 3.0).normalized();
        assert!((r.transform_dir(axis) - axis).length() < 1e-5);
    }

    #[test]
    fn reaparece_fuera_de_la_colision_y_en_rango_de_orbita() {
        let scene = Scene::new();
        for i in 1..=5 {
            let (pos, center, _) = respawn_point(&scene, i, 7.0);
            let r = scene.bodies[i].radius;
            let d = (pos - center).length();
            assert!(d > collision_radius(r) + 1.0);
            assert!(d - r < 2.5 * r, "fuera del rango de la orbita en el mundo {i}");
        }
    }
}
