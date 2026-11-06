//! FRAGMENT SHADERS escritos por nosotros. Cada uno es una struct con sus parámetros
//! ("uniforms" propios del objeto) que implementa `FragmentShader`.
//!
//! Para agregar un shader nuevo:
//!   1. Crear una struct con sus parámetros.
//!   2. `impl FragmentShader for MiShader { fn shade(...) -> Vec3 { ... } }`
//!   3. Usarlo al crear el planeta en `scene.rs`.

use crate::math::Vec3;
use crate::mesh::{sphere_dir, sphere_uv};
use crate::noise::{fbm, hash01, voronoi3};
use crate::normal_map::NormalMap;
use crate::renderer::{Fragment, FragmentShader, Uniforms};
use crate::texture::Texture;

// ---------------------------------------------------------------------------
// THEMYSCIRA: parámetros (el profe puede pedir cambiarlos en vivo).
// La "elevación" es el ruido fBm menos el nivel del mar: < 0 es agua, > 0 es tierra.
// ---------------------------------------------------------------------------
/// Frecuencia del ruido del terreno (más alta = islas más pequeñas y numerosas).
pub const THEMYSCIRA_FREQ: f32 = 3.0;
/// Nivel del mar en valores de fBm. Más alto = más océano y menos tierra.
pub const THEMYSCIRA_SEA_LEVEL: f32 = 0.05;
/// Hasta qué elevación hay playa (arena).
const THEMYSCIRA_BEACH_TOP: f32 = 0.025;
/// Desde qué elevación empieza la piedra clara (el pasto queda entre playa y piedra).
const THEMYSCIRA_STONE_START: f32 = 0.16;
/// Profundidad a la que el agua ya es azul profundo.
const THEMYSCIRA_DEEP: f32 = 0.15;
/// Ancho de la espuma blanca en la orilla (en elevación).
const THEMYSCIRA_FOAM: f32 = 0.012;
/// Brillo del sol sobre el mar (Blinn-Phong): intensidad y dureza.
const THEMYSCIRA_SPEC: f32 = 0.9;
const THEMYSCIRA_SHININESS: f32 = 90.0;
const THEMYSCIRA_DEEP_WATER: Vec3 = Vec3::new(0.02, 0.16, 0.42);
const THEMYSCIRA_SHALLOW_WATER: Vec3 = Vec3::new(0.10, 0.60, 0.72);
const THEMYSCIRA_SAND: Vec3 = Vec3::new(0.86, 0.79, 0.60);
const THEMYSCIRA_GRASS: Vec3 = Vec3::new(0.28, 0.52, 0.20);
const THEMYSCIRA_GRASS_DARK: Vec3 = Vec3::new(0.16, 0.36, 0.13);
const THEMYSCIRA_STONE: Vec3 = Vec3::new(0.86, 0.85, 0.80);

// ---------------------------------------------------------------------------
// KRYPTON: parámetros (el profe puede pedir cambiarlos en vivo).
// ---------------------------------------------------------------------------
/// Cuántas celdas Voronoi caben a lo ancho (más alto = cristales más pequeños).
pub const KRYPTON_CRYSTAL_SCALE: f32 = 7.0;
/// Cuánto se inclina la normal de cada faceta (0 = esfera lisa, 1 = muy facetado).
const KRYPTON_FACET_TILT: f32 = 0.55;
/// Ancho de las venas rojas (en unidades de f2 - f1).
const KRYPTON_VEIN_WIDTH: f32 = 0.08;
/// Intensidad del brillo rojo de las venas (> 1 satura hacia blanco en el centro).
const KRYPTON_VEIN_GLOW: f32 = 1.4;
/// Brillo especular de los cristales: intensidad y dureza.
const KRYPTON_SPEC: f32 = 1.2;
const KRYPTON_SHININESS: f32 = 60.0;
const KRYPTON_DEEP_BLUE: Vec3 = Vec3::new(0.04, 0.12, 0.42);
const KRYPTON_CYAN: Vec3 = Vec3::new(0.30, 0.72, 1.00);
const KRYPTON_VEIN_RED: Vec3 = Vec3::new(1.0, 0.12, 0.06);

/// Iluminación difusa (Lambert) desde el sol + luz ambiente.
pub fn lambert(frag: &Fragment, u: &Uniforms, ambient: f32) -> f32 {
    let l = (u.light_pos - frag.world_pos).normalized();
    ambient + (1.0 - ambient) * frag.normal.dot(l).max(0.0)
}

fn mix(a: Vec3, b: Vec3, t: f32) -> Vec3 {
    a.lerp(b, t.clamp(0.0, 1.0))
}

fn smoothstep(e0: f32, e1: f32, x: f32) -> f32 {
    let t = ((x - e0) / (e1 - e0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

// ---------------------------------------------------------------------------
// SOL: shader ANIMADO. Ruido fBm que se desplaza con el tiempo (manchas/plasma)
// y un borde más brillante. No recibe luz: emite la suya.
// ---------------------------------------------------------------------------
pub struct SunShader {
    pub seed: u32,
    /// Qué tan rápido "hierve" la superficie.
    pub speed: f32,
}

impl FragmentShader for SunShader {
    fn shade(&self, f: &Fragment, u: &Uniforms) -> Vec3 {
        let t = u.time * self.speed;
        let p = f.local_pos * 3.0;
        // Domain warping: el ruido desplaza las coordenadas de otro ruido → aspecto de plasma.
        let warp = Vec3::new(fbm(p + Vec3::new(t, 0.0, 0.0), 3, self.seed), fbm(p + Vec3::new(0.0, t, 0.0), 3, self.seed + 7), 0.0);
        let n = fbm(p + warp * 1.5 + Vec3::splat(t * 0.5), 4, self.seed + 13) * 0.5 + 0.5;

        let deep = Vec3::new(0.85, 0.25, 0.02);
        let mid = Vec3::new(1.0, 0.6, 0.1);
        let hot = Vec3::new(1.0, 0.95, 0.6);
        let mut c = if n < 0.5 { mix(deep, mid, n * 2.0) } else { mix(mid, hot, (n - 0.5) * 2.0) };

        // Borde brillante (fresnel): donde la normal es casi perpendicular a la vista.
        let view = (u.camera_pos - f.world_pos).normalized();
        let rim = 1.0 - f.normal.dot(view).max(0.0);
        c += Vec3::new(1.0, 0.5, 0.1) * rim.powf(3.0) * 0.8;
        c
    }
}

// ---------------------------------------------------------------------------
// THEMYSCIRA (Mujer Maravilla): islas de piedra clara y pasto en un mar Mediterráneo.
//
// Técnica: cada píxel se CLASIFICA por su elevación en un bioma (agua, playa, pasto o
// piedra) y cada bioma usa su propio modelo de luz:
//   - agua: Lambert + brillo especular Blinn-Phong del sol (el mar refleja como espejo),
//   - tierra: Lambert con la normal perturbada por el mapa de normales (relieve de rocas).
// La elevación es la MISMA función que deforma la malla en scene.rs, así que las costas
// pintadas coinciden con las islas reales. Usa local_pos para girar con el planeta.
// ---------------------------------------------------------------------------

/// Elevación del terreno en la dirección `dir` (unitaria, espacio del planeta).
/// < 0 = bajo el nivel del mar, > 0 = tierra. La usan este shader y la deformación del planeta.
pub fn themyscira_elevation(dir: Vec3, seed: u32) -> f32 {
    fbm(dir * THEMYSCIRA_FREQ, 5, seed) - THEMYSCIRA_SEA_LEVEL
}

pub struct ThemysciraShader {
    pub seed: u32,
    /// Relieve fino de las rocas (solo se aplica en tierra).
    pub normal_map: Option<NormalMap>,
    /// Intensidad del mapa de normales (0 = sin efecto, 1 = normal, 2 = exagerado).
    pub normal_strength: f32,
}

impl FragmentShader for ThemysciraShader {
    fn shade(&self, f: &Fragment, u: &Uniforms) -> Vec3 {
        let dir = f.local_pos.normalized();
        let e = themyscira_elevation(dir, self.seed);
        let l = (u.light_pos - f.world_pos).normalized();

        if e < 0.0 {
            // --- AGUA: color por profundidad + reflejo del sol ---
            let depth = -e;
            let mut water = mix(THEMYSCIRA_SHALLOW_WATER, THEMYSCIRA_DEEP_WATER, smoothstep(0.0, THEMYSCIRA_DEEP, depth));
            // Espuma: una franja blanca muy delgada pegada a la orilla.
            let foam = 1.0 - smoothstep(0.0, THEMYSCIRA_FOAM, depth);
            water = mix(water, Vec3::splat(0.9), foam * 0.6);

            let n = f.normal; // el mar es liso: normal de la esfera, sin mapa de normales
            let v = (u.camera_pos - f.world_pos).normalized();
            let h = (l + v).normalized();
            let diff = n.dot(l).max(0.0);
            // El brillo solo existe del lado iluminado (diff > 0).
            let spec = if diff > 0.0 { n.dot(h).max(0.0).powf(THEMYSCIRA_SHININESS) * THEMYSCIRA_SPEC } else { 0.0 };
            return water * (0.05 + 0.95 * diff) + Vec3::new(1.0, 0.95, 0.85) * spec;
        }

        // --- TIERRA: playa → pasto → piedra, con transiciones suaves ---
        let detail = fbm(dir * 30.0, 2, self.seed + 99) * 0.5 + 0.5; // manchas finas para verse de cerca
        let grass = mix(THEMYSCIRA_GRASS_DARK, THEMYSCIRA_GRASS, detail);
        let mut c = mix(THEMYSCIRA_SAND, grass, smoothstep(THEMYSCIRA_BEACH_TOP * 0.6, THEMYSCIRA_BEACH_TOP * 1.4, e));
        c = mix(c, THEMYSCIRA_STONE, smoothstep(THEMYSCIRA_STONE_START - 0.03, THEMYSCIRA_STONE_START + 0.03, e + (detail - 0.5) * 0.04));

        // Mapa de normales SOLO en tierra; entra poco a poco desde la orilla para no cortar.
        let mut land = *f;
        if u.normal_mapping {
            if let Some(map) = &self.normal_map {
                let strength = self.normal_strength * smoothstep(0.0, THEMYSCIRA_BEACH_TOP, e);
                land.normal = map.apply(f, strength);
            }
        }
        c * lambert(&land, u, 0.06)
    }
}

// ---------------------------------------------------------------------------
// KRYPTON (Superman): planeta de cristales azules con venas rojas brillantes.
//
// Técnica: ruido VORONOI (celular) sobre local_pos.
//   - Cada celda es una FACETA de cristal: con el id de la celda se saca un hash que
//     inclina la normal de toda la celda en su propia dirección (sobre la tangente y la
//     bitangente del planeta), así cada faceta refleja la luz distinto, como cristal tallado.
//   - Donde f2 - f1 es pequeño el píxel está en la frontera entre dos celdas: ahí se suma
//     ROJO EMISIVO (no depende del sol, brilla también del lado de noche).
//   - Especular Blinn-Phong fuerte + brillo de borde (fresnel) para que parezca cristal.
// ---------------------------------------------------------------------------
pub struct KryptonShader {
    pub seed: u32,
}

impl FragmentShader for KryptonShader {
    fn shade(&self, f: &Fragment, u: &Uniforms) -> Vec3 {
        let dir = f.local_pos.normalized();
        let (f1, f2, cell) = voronoi3(dir * KRYPTON_CRYSTAL_SCALE, self.seed);
        let rnd = |k: u32| hash01(cell.0, cell.1, cell.2, self.seed.wrapping_add(k));

        // --- Normal de la faceta: la normal del planeta inclinada al azar (fija por celda) ---
        let n = f.normal;
        let t = (f.tangent - n * n.dot(f.tangent)).normalized(); // tangente perpendicular a n
        let b = n.cross(t); // bitangente
        let tilt = (t * (rnd(10) * 2.0 - 1.0) + b * (rnd(11) * 2.0 - 1.0)) * KRYPTON_FACET_TILT;
        let facet = if t == Vec3::ZERO { n } else { (n + tilt).normalized() };

        // --- Color del cristal: cada faceta con su propio tono entre azul profundo y cian ---
        let tone = rnd(12);
        let mut base = mix(KRYPTON_DEEP_BLUE, KRYPTON_CYAN, tone * tone);
        base = base * (0.75 + 0.25 * (1.0 - f1).clamp(0.0, 1.0)); // más claro hacia el centro

        // --- Luz: Lambert + especular fuerte + borde de cristal ---
        let l = (u.light_pos - f.world_pos).normalized();
        let v = (u.camera_pos - f.world_pos).normalized();
        let h = (l + v).normalized();
        let diff = facet.dot(l).max(0.0);
        let spec = if diff > 0.0 { facet.dot(h).max(0.0).powf(KRYPTON_SHININESS) * KRYPTON_SPEC } else { 0.0 };
        let rim = (1.0 - n.dot(v).max(0.0)).powf(3.0) * 0.45;
        let crystal = base * (0.05 + 0.95 * diff) + Vec3::splat(spec) + KRYPTON_CYAN * rim;

        // --- Venas rojas emisivas en los bordes de las celdas ---
        let vein = 1.0 - smoothstep(0.0, KRYPTON_VEIN_WIDTH, f2 - f1);
        let glow = KRYPTON_VEIN_RED * KRYPTON_VEIN_GLOW + Vec3::splat(0.6) * vein.powf(6.0); // centro casi blanco
        mix(crystal, glow, vein)
    }
}

// ---------------------------------------------------------------------------
// NAVE: Blinn-Phong (difuso + brillo especular) + luz de borde para que se vea
// aunque esté del lado oscuro. Con el turbo, la parte de atrás se ilumina de azul.
// ---------------------------------------------------------------------------
pub struct ShipShader {
    pub base: Vec3,
    /// Turbo, de 0 a 1: la luz azul de la llama ilumina la parte trasera de la nave.
    pub boost: f32,
}

impl FragmentShader for ShipShader {
    fn shade(&self, f: &Fragment, u: &Uniforms) -> Vec3 {
        let l = (u.light_pos - f.world_pos).normalized();
        let v = (u.camera_pos - f.world_pos).normalized();
        let h = (l + v).normalized();
        let diff = f.normal.dot(l).max(0.0);
        let spec = f.normal.dot(h).max(0.0).powf(48.0);
        let rim = (1.0 - f.normal.dot(v).max(0.0)).powf(2.5) * 0.35;
        let mut c = self.base * (0.18 + 0.82 * diff) + Vec3::splat(spec * 0.7) + Vec3::new(0.3, 0.6, 1.0) * rim;

        // La llama del turbo ilumina la parte trasera de la nave (local +Z = atrás).
        let back = smoothstep(0.35, 0.7, f.local_pos.z);
        c += Vec3::new(0.25, 0.45, 0.9) * back * self.boost * 0.5;
        c
    }
}

// ---------------------------------------------------------------------------
// LLAMA DEL TURBO: shader ADITIVO y animado. Se dibuja con Renderer::draw_mesh_additive,
// así que su color se SUMA a lo que hay detrás (se ve como luz, no como un objeto sólido).
//   uv.1 = 0 en la boca del motor, 1 en la punta de la llama.
// ---------------------------------------------------------------------------
pub struct ExhaustShader {
    /// Intensidad del turbo (0 = invisible).
    pub boost: f32,
}

impl FragmentShader for ExhaustShader {
    fn shade(&self, f: &Fragment, u: &Uniforms) -> Vec3 {
        let t = f.uv.1.clamp(0.0, 1.0);
        // Más brillante donde la superficie mira a la cámara (el "centro" de la llama) y
        // se desvanece hacia los bordes, que así no se ven como un contorno duro.
        let v = (u.camera_pos - f.world_pos).normalized();
        let facing = f.normal.dot(v).abs();
        let body = facing.powf(1.5);
        // La llama es ancha y plana: sus extremos izquierdo/derecho (uv.0 cerca de 0 o 0.5,
        // donde |cos| ≈ 1) se apagan para que no se vea un borde recto.
        let side = 1.0 - smoothstep(0.55, 1.0, (f.uv.0 * std::f32::consts::TAU).cos().abs());
        // Se apaga hacia la punta.
        let fade = (1.0 - t).powf(1.8) * side;
        // Pulsos que viajan hacia atrás: la fase avanza con el tiempo.
        let pulse = 0.75 + 0.25 * (t * 22.0 - u.time * 38.0).sin();
        let noise = 0.85 + 0.3 * fbm(Vec3::new(f.uv.0 * 8.0, t * 5.0 - u.time * 9.0, 0.0), 2, 5);
        // Núcleo blanco-azul cerca del motor; azul más profundo hacia la punta.
        let core = Vec3::new(0.85, 0.95, 1.0);
        let outer = Vec3::new(0.15, 0.4, 1.0);
        let c = mix(core, outer, smoothstep(0.0, 0.6, t));
        c * (body * fade * pulse * noise * self.boost * 2.8)
    }
}

// ---------------------------------------------------------------------------
// ÓRBITA: línea punteada cuyos puntos avanzan en la dirección en que gira el planeta.
// uv.0 = distancia recorrida sobre la órbita (la calcula Renderer::draw_line_loop).
// No recibe luz: es una guía visual, como en un mapa.
// ---------------------------------------------------------------------------
pub struct OrbitShader {
    pub color: Vec3,
    /// Largo de cada tramo (punto + hueco), en unidades del mundo.
    pub dash_length: f32,
    /// Qué tan rápido avanzan los puntos (unidades del mundo por segundo). 0 = quietos.
    pub speed: f32,
}

impl FragmentShader for OrbitShader {
    fn shade(&self, f: &Fragment, u: &Uniforms) -> Vec3 {
        // Posición dentro del tramo actual, de 0 a 1.
        let s = ((f.uv.0 - u.time * self.speed) / self.dash_length).rem_euclid(1.0);
        // Primera mitad del tramo = punto brillante; segunda mitad = hueco tenue.
        let dash = if s < 0.5 { 1.0 } else { 0.2 };
        // A lo lejos se ve más tenue para no tapar el cielo.
        let dist = (f.world_pos - u.camera_pos).length();
        let fade = (1.0 - dist / 250.0).clamp(0.3, 1.0);
        self.color * dash * fade
    }
}

// ---------------------------------------------------------------------------
// CIELO (skybox procedural): se evalúa por dirección de vista en cada píxel vacío.
// Las estrellas están "pegadas" a la dirección, así que se ven en el horizonte y
// giran correctamente cuando gira la cámara (pero no se acercan al moverse).
//
// OPTIMIZACIÓN: la nebulosa (ruido fBm, lo más caro del cielo) es suave y no cambia con el
// tiempo, así que se calcula UNA vez al arrancar en una textura de 256x128 y en cada cuadro
// solo se lee. Antes el cielo costaba ~3.5 ms por cuadro.
// ---------------------------------------------------------------------------
pub struct Sky {
    nebula: Texture,
}

/// Color de la nebulosa tenue en una dirección del cielo.
fn nebula_color(dir: Vec3) -> Vec3 {
    let n = fbm(dir * 2.5, 3, 4242) * 0.5 + 0.5;
    Vec3::new(0.10, 0.03, 0.16) * smoothstep(0.55, 0.85, n) + Vec3::new(0.02, 0.06, 0.12) * smoothstep(0.5, 0.8, 1.0 - n) * 0.6
}

impl Sky {
    pub fn new() -> Sky {
        Sky { nebula: Texture::from_fn(256, 128, |u, v| nebula_color(sphere_dir(u, v))) }
    }

    /// Color del cielo en la dirección `dir` (se llama en cada píxel vacío).
    pub fn color(&self, dir: Vec3, u: &Uniforms) -> Vec3 {
        // Fondo + nebulosa leída de la textura precalculada (en vez de calcular el ruido).
        let (tu, tv) = sphere_uv(dir);
        let mut c = Vec3::new(0.005, 0.006, 0.02) + self.nebula.sample(tu, tv);

        // Estrellas: dividir el espacio en celdas; algunas celdas tienen una estrella.
        // Se calculan en vivo porque son baratas, finas (una textura las borraría) y titilan.
        for (scale, density, seed) in [(320.0, 0.020, 11u32), (140.0, 0.010, 23u32)] {
            let p = dir * scale;
            let cell = (p.x.floor() as i32, p.y.floor() as i32, p.z.floor() as i32);
            if hash01(cell.0, cell.1, cell.2, seed) < density {
                let jitter = Vec3::new(
                    hash01(cell.0, cell.1, cell.2, seed + 1),
                    hash01(cell.0, cell.1, cell.2, seed + 2),
                    hash01(cell.0, cell.1, cell.2, seed + 3),
                ) * 0.6 + Vec3::splat(0.2);
                let star = Vec3::new(cell.0 as f32, cell.1 as f32, cell.2 as f32) + jitter;
                let d = (p - star).length();
                let twinkle = 0.75 + 0.25 * (u.time * 3.0 + hash01(cell.0, cell.1, cell.2, seed + 4) * 50.0).sin();
                let b = (1.0 - d / 0.6).max(0.0).powf(1.5) * twinkle;
                let tint = hash01(cell.0, cell.1, cell.2, seed + 5);
                c += mix(Vec3::new(0.7, 0.8, 1.0), Vec3::new(1.0, 0.85, 0.6), tint) * b;
            }
        }

        // Resplandor alrededor del sol (depende de la cámara, así que también va en vivo).
        let to_sun = (u.light_pos - u.camera_pos).normalized();
        let g = dir.dot(to_sun).max(0.0);
        c += Vec3::new(1.0, 0.55, 0.15) * (g.powf(60.0) * 0.6 + g.powf(8.0) * 0.08);

        // Efecto warp (solo cuesta cuando está activo).
        if u.warp > 0.0 {
            c += warp_streaks(dir, u) * u.warp;
        }
        c
    }
}

// ---------------------------------------------------------------------------
// WARP: shader ANIMADO de "túnel de velocidad". Rayas de luz que salen del centro de la
// vista y avanzan hacia afuera con el tiempo. Se calcula en coordenadas de la cámara:
//   - ángulo alrededor del centro → en qué raya cae el píxel
//   - "profundidad del túnel" = z / r → dónde va el tramo de luz a lo largo de la raya
// ---------------------------------------------------------------------------
/// Cuántas rayas posibles hay alrededor del centro (más = más finas y juntas).
const WARP_SECTORS: f32 = 240.0;
/// Qué tan rápido avanzan los tramos de luz hacia afuera.
const WARP_SPEED: f32 = 4.0;

fn warp_streaks(dir: Vec3, u: &Uniforms) -> Vec3 {
    // Dirección del píxel en la base de la cámara.
    let x = dir.dot(u.camera_right);
    let y = dir.dot(u.camera_up);
    let z = dir.dot(u.camera_forward);
    let r = (x * x + y * y).sqrt().max(1e-4); // distancia al centro de la vista

    // Tinte azul que crece hacia los bordes de la pantalla.
    let tint = Vec3::new(0.04, 0.08, 0.25) * smoothstep(0.2, 0.8, r);

    // 1. ¿En qué sector (raya) cae el píxel? Solo algunos sectores, elegidos con hash, tienen raya.
    let s = (y.atan2(x) / std::f32::consts::TAU + 0.5) * WARP_SECTORS;
    let h = hash01(s.floor() as i32, 0, 0, 777);
    if h > 0.45 {
        return tint;
    }
    // Raya delgada: brilla solo cerca del centro del sector.
    let line = 1.0 - smoothstep(0.05, 0.35, (s - s.floor() - 0.5).abs());

    // 2. Tramos de luz que se mueven: la fase depende de la profundidad y del TIEMPO.
    //    Al crecer el tiempo, un mismo valor de fase ocurre a menor profundidad → se mueve hacia afuera.
    let depth = z / r;
    let phase = (depth * 0.6 + u.time * WARP_SPEED + h * 20.0).rem_euclid(1.0);
    let dash = smoothstep(0.0, 0.1, phase) * (1.0 - smoothstep(0.3, 0.6, phase));

    // 3. En el centro las rayas se juntan y se verían como ruido: las apagamos ahí.
    let edge = smoothstep(0.1, 0.5, r);
    tint + Vec3::new(0.55, 0.75, 1.0) * (line * dash * edge * 1.5)
}
