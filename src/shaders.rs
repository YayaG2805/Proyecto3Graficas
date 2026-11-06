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

// ---------------------------------------------------------------------------
// GOTHAM: parámetros (el profe puede pedir cambiarlos en vivo).
// Paleta a propósito apagada: grises, asfalto y agua casi negra; solo las luces de noche brillan.
// ---------------------------------------------------------------------------
/// Manzanas alrededor del planeta (longitud) y de polo a polo (latitud).
pub const GOTHAM_BLOCKS_U: f32 = 64.0;
pub const GOTHAM_BLOCKS_V: f32 = 32.0;
/// Fracción de cada manzana que es calle (se reparte en los dos bordes).
const GOTHAM_STREET: f32 = 0.16;
/// Cada cuántas manzanas hay una AVENIDA (calle ancha con tráfico), en cada dirección.
const GOTHAM_AVENUE_EVERY: i32 = 6;
/// Ancho de la avenida, en fracción de la manzana.
const GOTHAM_AVENUE_WIDTH: f32 = 0.34;
/// Latitud (radianes) desde la que empieza la bahía de los polos (~66°).
pub const GOTHAM_POLAR_LIMIT: f32 = 1.15;
/// RÍO: frecuencia (más alto = más curvas y más ríos) y ancho (en unidades de |fbm|).
const GOTHAM_RIVER_FREQ: f32 = 1.6;
const GOTHAM_RIVER_WIDTH: f32 = 0.018;
/// LAGOS: frecuencia y nivel (más bajo = lagos más grandes).
const GOTHAM_LAKE_FREQ: f32 = 2.2;
const GOTHAM_LAKE_LEVEL: f32 = 0.24;
/// CARRETERAS curvas: frecuencia y ancho (en unidades de |fbm|).
const GOTHAM_HIGHWAY_FREQ: f32 = 1.1;
const GOTHAM_HIGHWAY_WIDTH: f32 = 0.011;
/// Probabilidad de que una manzana sea parque.
const GOTHAM_PARK_CHANCE: f32 = 0.06;
/// Ventanas por lado en cada azotea (4 → 4×4 ventanas).
const GOTHAM_WINDOWS: f32 = 4.0;
/// Fracción de ventanas encendidas (0 = ciudad apagada, 1 = todas prendidas).
const GOTHAM_WINDOW_DENSITY: f32 = 0.45;
/// Qué tan seguido puede cambiar cada ventana (veces por segundo). Bajo = parpadeo lento.
const GOTHAM_FLICKER_SPEED: f32 = 0.1;
/// Velocidad de las luces de los carros en avenidas y carreteras.
const GOTHAM_TRAFFIC_SPEED: f32 = 1.5;
/// Fracción de tramos de avenida/carretera con un carro (0 = vacías).
const GOTHAM_TRAFFIC_DENSITY: f32 = 0.35;
/// Por debajo de esta luz del sol (Lambert) se considera de noche y se encienden las luces.
const GOTHAM_NIGHT: f32 = 0.15;
/// Luz ambiente muy baja: la ciudad casi siempre se ve de noche.
const GOTHAM_AMBIENT: f32 = 0.03;
const GOTHAM_CONCRETE: Vec3 = Vec3::new(0.30, 0.30, 0.33);
const GOTHAM_GLASS: Vec3 = Vec3::new(0.06, 0.07, 0.09);
const GOTHAM_ASPHALT: Vec3 = Vec3::new(0.07, 0.07, 0.08);
const GOTHAM_HIGHWAY: Vec3 = Vec3::new(0.12, 0.12, 0.13);
const GOTHAM_LANE_LINE: Vec3 = Vec3::new(0.22, 0.20, 0.12); // ocre apagado
const GOTHAM_PARK: Vec3 = Vec3::new(0.10, 0.13, 0.09); // verde muy oscuro y desaturado
const GOTHAM_WATER: Vec3 = Vec3::new(0.03, 0.045, 0.06); // pizarra casi negra
const GOTHAM_WINDOW: Vec3 = Vec3::new(0.95, 0.74, 0.36);
const GOTHAM_STREETLAMP: Vec3 = Vec3::new(0.85, 0.45, 0.15);
const GOTHAM_HEADLIGHT: Vec3 = Vec3::new(0.60, 0.58, 0.50);
const GOTHAM_TAILLIGHT: Vec3 = Vec3::new(0.38, 0.07, 0.05);

// ---------------------------------------------------------------------------
// ATLANTIS: parámetros (el profe puede pedir cambiarlos en vivo).
// ---------------------------------------------------------------------------
/// Qué tan rápido avanzan las olas.
pub const WAVE_SPEED: f32 = 1.0;
/// Qué tan altas se "sienten" las olas (cuánto inclinan la normal). 0 = mar plano.
pub const WAVE_HEIGHT: f32 = 0.006;
/// Cuántas olas caben alrededor del planeta (más alto = olas más cortas y numerosas).
pub const WAVE_FREQ: f32 = 40.0;
/// Las 3 ondas: (dirección, multiplicador de frecuencia, multiplicador de velocidad, amplitud).
const ATLANTIS_WAVES: [(Vec3, f32, f32, f32); 3] = [
    (Vec3::new(0.94, 0.19, 0.28), 1.0, 1.0, 0.5),
    (Vec3::new(-0.40, 0.91, 0.10), 1.7, 1.3, 0.3),
    (Vec3::new(0.19, -0.29, 0.94), 2.6, 0.8, 0.2),
];
/// Desde qué altura del fondo marino (fBm) aparecen arrecifes. Más bajo = más arrecifes.
pub const ATLANTIS_REEF_LEVEL: f32 = 0.20;
/// Brillo del sol en las olas (Blinn-Phong): intensidad y dureza.
const ATLANTIS_SPEC: f32 = 1.0;
const ATLANTIS_SHININESS: f32 = 220.0;
/// Cuánto doblan el fBm los frentes de ola (radianes de fase). 0 = olas rectas y repetidas.
const ATLANTIS_WAVE_WARP: f32 = 7.0;
const ATLANTIS_DEEP: Vec3 = Vec3::new(0.01, 0.06, 0.20);
const ATLANTIS_SHALLOW: Vec3 = Vec3::new(0.05, 0.45, 0.55);
const ATLANTIS_REEF_PINK: Vec3 = Vec3::new(0.72, 0.36, 0.45);
const ATLANTIS_REEF_GREEN: Vec3 = Vec3::new(0.24, 0.52, 0.34);
const ATLANTIS_SKY: Vec3 = Vec3::new(0.45, 0.65, 0.85);

// ---------------------------------------------------------------------------
// OA: parámetros (el profe puede pedir cambiarlos en vivo).
// ---------------------------------------------------------------------------
/// Cuántos anillos de energía hay de polo a polo (frecuencia en la latitud).
const OA_RINGS: f32 = 20.0;
/// Qué tan rápido viajan los anillos de un polo al otro.
const OA_RING_SPEED: f32 = 3.0;
/// Qué tan delgados son los anillos (exponente: más alto = más finos).
const OA_RING_SHARPNESS: f32 = 10.0;
/// Velocidad de la "respiración" del brillo de borde.
const OA_BREATH_SPEED: f32 = 1.2;
/// Celdas de la cuadrícula de circuitos (alrededor y de polo a polo).
const OA_GRID_U: f32 = 96.0;
const OA_GRID_V: f32 = 48.0;
/// Probabilidad de que una celda tenga pista horizontal / vertical.
const OA_LINE_DENSITY: f32 = 0.5;
/// Grosor de las pistas, en fracción de la celda.
const OA_LINE_WIDTH: f32 = 0.08;
/// Probabilidad de que una celda tenga un nodo (punto brillante).
const OA_NODE_CHANCE: f32 = 0.12;
const OA_BASE: Vec3 = Vec3::new(0.02, 0.07, 0.03);
const OA_CIRCUIT: Vec3 = Vec3::new(0.12, 0.80, 0.28);
const OA_RING_GLOW: Vec3 = Vec3::new(0.45, 1.0, 0.45);
const OA_RIM: Vec3 = Vec3::new(0.25, 1.0, 0.35);

// ---------------------------------------------------------------------------
// CENTRAL CITY (luna de Gotham, Flash): parámetros de los rayos (Diego, D4).
// ---------------------------------------------------------------------------
/// Cuántos rayos independientes puede haber a la vez.
const CC_BOLTS: i32 = 4;
/// Cada cuántos segundos vuelve a caer cada rayo (con desfase propio).
const CC_BOLT_PERIOD: f32 = 3.0;
/// Qué fracción del periodo dura el rayo encendido (el resto la luna queda tranquila).
const CC_BOLT_ACTIVE: f32 = 0.3;
/// Largo del arco que recorre el rayo sobre la luna (radianes; π = media vuelta).
const CC_BOLT_ARC: f32 = 2.6;
/// Grosor del núcleo del rayo (en distancia sobre la esfera de radio 1).
const CC_BOLT_WIDTH: f32 = 0.045;
/// Qué tanto zigzaguea el rayo y qué tan finos son los quiebres.
const CC_BOLT_JAG: f32 = 0.07;
const CC_BOLT_JAG_FREQ: f32 = 9.0;
const CC_ROCK_DARK: Vec3 = Vec3::new(0.16, 0.15, 0.17);
const CC_ROCK_LIGHT: Vec3 = Vec3::new(0.38, 0.36, 0.33);
const CC_BOLT_CORE: Vec3 = Vec3::new(1.0, 0.97, 0.75);
const CC_BOLT_GLOW: Vec3 = Vec3::new(1.0, 0.78, 0.10);

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
// GOTHAM (Batman): planeta-ciudad oscuro. Shader ANIMADO.
//
// Técnica: CUADRÍCULA DE MANZANAS sobre las coordenadas (u, v) de la esfera, más capas de
// ruido que la "rompen" para que parezca un mapa de ciudad:
//   - La parte entera de (u·BLOCKS_U, v·BLOCKS_V) es el id de la manzana; la parte decimal
//     dice dónde está el píxel dentro de ella: en los bordes = calle, en el centro = azotea.
//   - RÍO y CARRETERAS: donde un fBm cruza por cero (|fbm| < ancho) sale una línea curva que
//     recorre el planeta. LAGOS: donde otro fBm supera un nivel. Las carreteras se revisan
//     antes que el agua, así que al cruzar un río se ven encima, como un puente.
//   - AVENIDAS cada N manzanas y PARQUES en algunas manzanas al azar (hash).
//   - DÍA (lado iluminado): todo con Lambert y colores apagados.
//   - NOCHE (Lambert < GOTHAM_NIGHT): ventanas EMISIVAS encendidas al azar que cambian cada
//     pocos segundos, faroles en calles y carreteras, y luces de carros que se MUEVEN con time.
//   - Cerca de los polos las manzanas se aprietan (así es sphere_uv): ahí hay una bahía.
// ---------------------------------------------------------------------------

/// Qué hay en un punto de Gotham.
#[derive(Clone, Copy, PartialEq)]
pub enum GothamKind {
    /// Bahía de los polos, río o lago.
    Water,
    /// Carretera curva. `side` = lado de la línea central (-1 o 1), para el sentido del tráfico.
    Highway { center: bool, side: f32 },
    /// Avenida. `along` = coordenada a lo largo de ella (para mover los carros), `side` = sentido.
    Avenue { along: f32, side: f32 },
    Street,
    Park,
    /// Azotea de un edificio.
    Roof,
}

/// Punto de Gotham: su manzana y qué hay ahí.
pub struct GothamCell {
    /// (columna, fila) de la manzana: su "id".
    pub id: (i32, i32),
    /// Posición dentro de la manzana, de 0 a 1 en cada eje.
    pub fu: f32,
    pub fv: f32,
    pub kind: GothamKind,
}

/// ¿La manzana `id` es un parque? (por hash: siempre la misma respuesta para la misma manzana)
fn gotham_is_park(id: (i32, i32), seed: u32) -> bool {
    hash01(id.0, id.1, 5, seed) < GOTHAM_PARK_CHANCE
}

/// Clasifica una dirección (unitaria, espacio del planeta). La usan el shader y la malla.
pub fn gotham_cell(dir: Vec3, seed: u32) -> GothamCell {
    let (u, v) = sphere_uv(dir);
    let (bu, bv) = (u * GOTHAM_BLOCKS_U, v * GOTHAM_BLOCKS_V);
    // rem_euclid: u = 1 es lo mismo que u = 0 (da la vuelta), así no hay costura.
    let id = ((bu.floor() as i32).rem_euclid(GOTHAM_BLOCKS_U as i32), bv.floor() as i32);
    let (fu, fv) = (bu - bu.floor(), bv - bv.floor());

    // 1. Carreteras (primero: pasan por encima del agua como puentes).
    let hw = fbm(dir * GOTHAM_HIGHWAY_FREQ, 3, seed.wrapping_add(50));
    let kind = if hw.abs() < GOTHAM_HIGHWAY_WIDTH {
        GothamKind::Highway { center: hw.abs() < GOTHAM_HIGHWAY_WIDTH * 0.06, side: hw.signum() }
    }
    // 2. Agua: bahía en los polos, río (línea curva) y lagos (manchas).
    else if dir.y.clamp(-1.0, 1.0).asin().abs() > GOTHAM_POLAR_LIMIT
        || fbm(dir * GOTHAM_RIVER_FREQ, 4, seed.wrapping_add(20)).abs() < GOTHAM_RIVER_WIDTH
        || fbm(dir * GOTHAM_LAKE_FREQ, 3, seed.wrapping_add(30)) > GOTHAM_LAKE_LEVEL
    {
        GothamKind::Water
    }
    // 3. Avenidas: borde izquierdo de cada N columnas (tráfico a lo largo de v) y borde
    //    superior de cada N filas (tráfico a lo largo de u).
    else if id.0 % GOTHAM_AVENUE_EVERY == 0 && fu < GOTHAM_AVENUE_WIDTH {
        GothamKind::Avenue { along: id.1 as f32 + fv, side: if fu < GOTHAM_AVENUE_WIDTH * 0.5 { -1.0 } else { 1.0 } }
    } else if id.1 % GOTHAM_AVENUE_EVERY == 0 && fv < GOTHAM_AVENUE_WIDTH {
        GothamKind::Avenue { along: id.0 as f32 + fu, side: if fv < GOTHAM_AVENUE_WIDTH * 0.5 { -1.0 } else { 1.0 } }
    }
    // 4. Calles normales en los bordes de cada manzana.
    else if fu < GOTHAM_STREET * 0.5 || fu > 1.0 - GOTHAM_STREET * 0.5 || fv < GOTHAM_STREET * 0.5 || fv > 1.0 - GOTHAM_STREET * 0.5 {
        GothamKind::Street
    } else if gotham_is_park(id, seed) {
        GothamKind::Park
    } else {
        GothamKind::Roof
    };
    GothamCell { id, fu, fv, kind }
}

/// Altura (0..1) del terreno en esa dirección. La usa scene.rs para subir la malla en
/// escalones. hash³: la mayoría de manzanas son bajas y unas pocas son torres altas.
/// Agua a nivel 0; carreteras y parques casi al ras.
pub fn gotham_height(dir: Vec3, seed: u32) -> f32 {
    let c = gotham_cell(dir, seed);
    match c.kind {
        GothamKind::Water => 0.0,
        GothamKind::Highway { .. } => 0.05,
        _ if gotham_is_park(c.id, seed) => 0.05,
        _ => {
            let h = hash01(c.id.0, c.id.1, 0, seed);
            0.15 + 0.85 * h * h * h
        }
    }
}

/// Luces de carros que avanzan por `along` (en el sentido de `side`). Devuelve el color de la
/// luz o negro. Cada "auto" ocupa un tramo corto; algunos tramos van vacíos (hash).
fn gotham_traffic(along: f32, side: f32, time: f32, seed: u32) -> Vec3 {
    // Un sentido avanza y el otro retrocede: faros blancos de frente, luces rojas de espalda.
    let s = along * 6.0 - side * time * GOTHAM_TRAFFIC_SPEED;
    let slot = s.floor() as i32;
    let car = (s - s.floor()) < 0.18 && hash01(slot, side as i32, 7, seed) < GOTHAM_TRAFFIC_DENSITY;
    if !car {
        Vec3::ZERO
    } else if side > 0.0 {
        GOTHAM_HEADLIGHT
    } else {
        GOTHAM_TAILLIGHT
    }
}

pub struct GothamShader {
    pub seed: u32,
}

impl FragmentShader for GothamShader {
    fn shade(&self, f: &Fragment, u: &Uniforms) -> Vec3 {
        let dir = f.local_pos.normalized();
        let c = gotham_cell(dir, self.seed);
        let l = (u.light_pos - f.world_pos).normalized();
        let diff = f.normal.dot(l).max(0.0);
        let light = GOTHAM_AMBIENT + (1.0 - GOTHAM_AMBIENT) * diff;
        // 0 = noche cerrada, 1 = día. Transición suave alrededor del umbral.
        let night = 1.0 - smoothstep(GOTHAM_NIGHT - 0.08, GOTHAM_NIGHT + 0.08, diff);
        let grime = 0.8 + 0.2 * fbm(dir * 40.0, 2, self.seed); // manchas de hollín

        match c.kind {
            GothamKind::Water => {
                // Agua casi negra con un reflejo pequeño del sol (solo de día).
                let v = (u.camera_pos - f.world_pos).normalized();
                let spec = f.normal.dot((l + v).normalized()).max(0.0).powf(60.0) * 0.35 * (1.0 - night);
                GOTHAM_WATER * light + Vec3::splat(spec)
            }
            GothamKind::Highway { center, side } => {
                let surface = if center { GOTHAM_LANE_LINE } else { GOTHAM_HIGHWAY * grime };
                // Coordenada "a lo largo" aproximada: otro ruido que cruza la carretera.
                let along = fbm(dir * 3.0, 2, self.seed.wrapping_add(51)) * 40.0;
                let lights = GOTHAM_STREETLAMP * 0.10 + gotham_traffic(along, side, u.time, self.seed);
                surface * light + lights * night
            }
            GothamKind::Avenue { along, side } => {
                let lights = GOTHAM_STREETLAMP * 0.08 + gotham_traffic(along, side, u.time, self.seed);
                GOTHAM_ASPHALT * grime * light + lights * night
            }
            GothamKind::Street => GOTHAM_ASPHALT * light + GOTHAM_STREETLAMP * (0.05 * night),
            GothamKind::Park => {
                // Copas de árboles: manchas de ruido fino sobre verde muy oscuro.
                let trees = 0.6 + 0.6 * (fbm(dir * 90.0, 2, self.seed.wrapping_add(9)) * 0.5 + 0.5);
                GOTHAM_PARK * trees * light
            }
            GothamKind::Roof => {
                // Coordenadas dentro de la azotea (sin la calle), de 0 a 1, y la cuadrícula de ventanas.
                let half = GOTHAM_STREET * 0.5;
                let ru = (c.fu - half) / (1.0 - GOTHAM_STREET);
                let rv = (c.fv - half) / (1.0 - GOTHAM_STREET);
                let (wx, wy) = (ru * GOTHAM_WINDOWS, rv * GOTHAM_WINDOWS);
                // Marco: solo el centro de cada celda es vidrio (deja un borde de concreto).
                let (cx, cy) = (wx - wx.floor(), wy - wy.floor());
                let glass = cx > 0.2 && cx < 0.8 && cy > 0.2 && cy < 0.8;
                let tone = hash01(c.id.0, c.id.1, 1, self.seed); // gris propio de cada manzana
                let surface = if glass { GOTHAM_GLASS } else { GOTHAM_CONCRETE * (0.6 + 0.6 * tone) * grime };

                // Ventanas encendidas: cada una "tira el dado" en momentos distintos (fase propia).
                let wid = (c.id.0 * 16 + wx.floor() as i32, c.id.1 * 16 + wy.floor() as i32);
                let phase = hash01(wid.0, wid.1, 2, self.seed) * 10.0;
                let epoch = (u.time * GOTHAM_FLICKER_SPEED + phase).floor() as i32;
                let on = glass && hash01(wid.0, wid.1, epoch, self.seed.wrapping_add(3)) < GOTHAM_WINDOW_DENSITY;
                let emit = if on { GOTHAM_WINDOW * (0.6 + 0.4 * hash01(wid.0, wid.1, 4, self.seed)) } else { Vec3::ZERO };
                surface * light + emit * night
            }
        }
    }
}

// ---------------------------------------------------------------------------
// ATLANTIS (Aquaman): océano con olas en movimiento y arrecifes. Shader ANIMADO.
//
// Técnica: la malla es una esfera LISA; las olas están en la NORMAL.
//   1. Altura de las olas = suma de 3 ondas senoidales que avanzan en direcciones distintas:
//        h = Σ aᵢ · sin(dir·kᵢ · WAVE_FREQ · fᵢ + warp − time · WAVE_SPEED · sᵢ)
//      `warp` es un fBm suave que DOBLA los frentes de ola (si no, 3 ondas rectas cruzadas se
//      ven como una cuadrícula), y otro fBm lento cambia la amplitud de un lugar a otro.
//   2. Su GRADIENTE (derivada) dice hacia dónde sube la ola. Se proyecta en el plano tangente
//      y se inclina la normal en contra: n = normalize(N − (T·gt + B·gb) · WAVE_HEIGHT).
//      Es la misma base TBN del mapeo de normales, pero con ondas que se mueven con time.
//   3. Color por profundidad (ruido de "fondo marino"): azul marino → turquesa, y arrecifes
//      rosados/verdes donde el fondo está más alto.
//   4. Luz: Lambert + Blinn-Phong con la normal de las olas → el reflejo del sol brilla y se
//      mueve; borde (fresnel) con el color del cielo, como el agua vista de lado.
// ---------------------------------------------------------------------------
/// "Altura" del fondo marino de Atlantis en la dirección `dir` (fija, no cambia con el tiempo).
/// Donde supera ATLANTIS_REEF_LEVEL hay arrecifes: la usan el shader y el reparto de corales.
pub fn atlantis_seabed(dir: Vec3, seed: u32) -> f32 {
    fbm(dir * 2.5, 4, seed)
}

pub struct AtlantisShader {
    pub seed: u32,
}

impl FragmentShader for AtlantisShader {
    fn shade(&self, f: &Fragment, u: &Uniforms) -> Vec3 {
        let dir = f.local_pos.normalized();

        // --- 1 y 2. Olas: altura y gradiente (en espacio del planeta) ---
        // El fBm se desplaza despacio con el tiempo: zonas de mar más picado y más calmado.
        let choppy = 0.7 + 0.6 * (fbm(dir * 4.0 + Vec3::splat(u.time * 0.05), 2, self.seed.wrapping_add(40)) * 0.5 + 0.5);
        let warp = fbm(dir * 3.0, 2, self.seed.wrapping_add(42)) * ATLANTIS_WAVE_WARP;
        let mut height = 0.0;
        let mut grad = Vec3::ZERO;
        for (k, fk, sk, a) in ATLANTIS_WAVES {
            let freq = WAVE_FREQ * fk;
            // (El warp cambia despacio en el espacio, así que su derivada se ignora: es pequeña.)
            let phase = dir.dot(k) * freq + warp * fk - u.time * WAVE_SPEED * sk;
            height += a * phase.sin();
            grad += k * (a * phase.cos() * freq); // derivada de a·sin(dir·k·freq) respecto a dir
        }
        let grad = grad * choppy;

        // Base tangente local (igual a la tangente de la icosfera) y la misma en el mundo.
        let n = f.normal;
        let t_world = (f.tangent - n * n.dot(f.tangent)).normalized();
        let wave_n = if t_world == Vec3::ZERO {
            n // en los polos no hay "este": sin olas
        } else {
            let t_local = Vec3::new(-dir.z, 0.0, dir.x).normalized();
            let b_local = dir.cross(t_local);
            let (gt, gb) = (grad.dot(t_local), grad.dot(b_local)); // gradiente en el plano tangente
            (n - (t_world * gt + n.cross(t_world) * gb) * WAVE_HEIGHT).normalized()
        };

        // --- 3. Color: profundidad y arrecifes ---
        let seabed = atlantis_seabed(dir, self.seed); // "altura" del fondo marino (fija)
        let mut water = mix(ATLANTIS_DEEP, ATLANTIS_SHALLOW, smoothstep(-0.05, 0.25, seabed));
        let reef = smoothstep(ATLANTIS_REEF_LEVEL, ATLANTIS_REEF_LEVEL + 0.05, seabed);
        if reef > 0.0 {
            let pink_or_green = fbm(dir * 25.0, 2, self.seed.wrapping_add(41)) * 0.5 + 0.5;
            let coral = mix(ATLANTIS_REEF_GREEN, ATLANTIS_REEF_PINK, smoothstep(0.4, 0.6, pink_or_green));
            water = mix(water, coral, reef * 0.6); // se ve a través del agua: no del todo
        }
        // Espuma suave en las crestas más altas.
        let crest = smoothstep(0.65, 1.0, height * choppy);
        water = mix(water, Vec3::splat(0.75), crest * 0.18);

        // --- 4. Luz ---
        let l = (u.light_pos - f.world_pos).normalized();
        let v = (u.camera_pos - f.world_pos).normalized();
        let h = (l + v).normalized();
        let sun = n.dot(l).max(0.0); // lado iluminado (con la normal lisa)
        let diff = wave_n.dot(l).max(0.0);
        // Dos brillos: uno amplio con la normal lisa (el "camino" del sol en el mar) y destellos
        // pequeños y muy duros con la normal de las olas (se mueven con ellas).
        let lit = smoothstep(0.0, 0.1, sun);
        let spec = (n.dot(h).max(0.0).powf(25.0) * 0.12 + wave_n.dot(h).max(0.0).powf(ATLANTIS_SHININESS) * ATLANTIS_SPEC) * lit;
        let fresnel = (1.0 - wave_n.dot(v).max(0.0)).powf(4.0) * 0.4 * (0.2 + 0.8 * sun);
        water * (0.04 + 0.96 * diff) + Vec3::new(1.0, 0.95, 0.85) * spec + ATLANTIS_SKY * fresnel
    }
}

// ---------------------------------------------------------------------------
// OA (Linterna Verde): planeta verde que pulsa con energía. Shader ANIMADO y casi todo EMISIVO.
//
// Técnica: patrones geométricos que dependen de la LATITUD y del ÁNGULO DE VISIÓN.
//   - Circuitos: cuadrícula sobre sphere_uv; en cada celda un hash decide si hay pista
//     horizontal, vertical o ninguna (así salen tramos que se cortan, no una rejilla pareja)
//     y a veces un nodo brillante en el centro.
//   - Anillos de energía: sin(latitud × OA_RINGS − time × OA_RING_SPEED) → bandas que viajan
//     de un polo al otro; elevadas a una potencia quedan como anillos finos. Al pasar un
//     anillo, los circuitos se encienden más.
//   - Borde (fresnel) verde que "respira": su intensidad sube y baja con sin(time).
//   - Base verde muy oscura con un Lambert suave (para notar de qué lado está el sol).
// ---------------------------------------------------------------------------
pub struct OaShader {
    pub seed: u32,
}

impl FragmentShader for OaShader {
    fn shade(&self, f: &Fragment, u: &Uniforms) -> Vec3 {
        let dir = f.local_pos.normalized();
        let lat = dir.y.clamp(-1.0, 1.0).asin();

        // --- Anillos de energía que viajan de polo a polo ---
        let wave = (lat * OA_RINGS - u.time * OA_RING_SPEED).sin() * 0.5 + 0.5;
        let ring = wave.powf(OA_RING_SHARPNESS);

        // --- Circuitos: pistas por celda ---
        let (cu, cv) = sphere_uv(dir);
        let (gu, gv) = (cu * OA_GRID_U, cv * OA_GRID_V);
        let cell = ((gu.floor() as i32).rem_euclid(OA_GRID_U as i32), gv.floor() as i32);
        let (x, y) = (gu - gu.floor() - 0.5, gv - gv.floor() - 0.5); // -0.5..0.5 dentro de la celda
        let has_h = hash01(cell.0, cell.1, 1, self.seed) < OA_LINE_DENSITY;
        let has_v = hash01(cell.0, cell.1, 2, self.seed) < OA_LINE_DENSITY;
        let on_h = has_h && y.abs() < OA_LINE_WIDTH;
        let on_v = has_v && x.abs() < OA_LINE_WIDTH;
        let trace = if on_h || on_v { 1.0 } else { 0.0 };
        // Nodo: punto redondo en el centro de algunas celdas (o donde se cruzan dos pistas).
        let is_node = (has_h && has_v) || hash01(cell.0, cell.1, 3, self.seed) < OA_NODE_CHANCE;
        let node = if is_node { 1.0 - smoothstep(0.12, 0.2, (x * x + y * y).sqrt()) } else { 0.0 };
        // Cerca de los polos las celdas se aprietan: ahí se apagan los circuitos.
        let polar = 1.0 - smoothstep(1.1, 1.35, lat.abs());

        // --- Borde que respira ---
        let v = (u.camera_pos - f.world_pos).normalized();
        let breath = 0.6 + 0.4 * (u.time * OA_BREATH_SPEED).sin();
        let rim = (1.0 - f.normal.dot(v).max(0.0)).powf(3.0) * breath;

        // --- Suma: base iluminada + todo lo emisivo ---
        let l = (u.light_pos - f.world_pos).normalized();
        let base = OA_BASE * (0.15 + 0.85 * f.normal.dot(l).max(0.0));
        let circuits = OA_CIRCUIT * (trace * (0.35 + 1.2 * ring)) + OA_RING_GLOW * (node * (0.5 + ring));
        base + circuits * polar + OA_RING_GLOW * (ring * 0.3) + OA_RIM * (rim * 0.9)
    }
}

// ---------------------------------------------------------------------------
// CENTRAL CITY: shader ANIMADO de superficie eléctrica (Diego, D4).
// Roca gris iluminada con Lambert + rayos amarillos EMISIVOS que cruzan la luna.
// Cada rayo i se repite cada CC_BOLT_PERIOD segundos; en cada repetición ("época")
// elige al azar un círculo máximo de la esfera y avanza sobre él:
//   - distancia al plano del círculo = |p · eje|, con zigzag de ruido → qué tan cerca del rayo
//   - ángulo dentro del plano → qué parte del arco ya recorrió la punta del rayo
// ---------------------------------------------------------------------------
pub struct CentralCityShader {
    pub seed: u32,
}

/// Dirección al azar (uniforme sobre la esfera) a partir de dos números del hash.
fn random_dir(a: f32, b: f32) -> Vec3 {
    let y = a * 2.0 - 1.0;
    let ang = b * std::f32::consts::TAU;
    let r = (1.0 - y * y).max(0.0).sqrt();
    Vec3::new(r * ang.cos(), y, r * ang.sin())
}

impl CentralCityShader {
    /// Brillo del rayo `i` en el punto `p` (sobre la esfera de radio 1) en el tiempo `t`.
    fn bolt(&self, i: i32, p: Vec3, t: f32) -> f32 {
        // Época y avance: cada rayo tiene su propio desfase para que no caigan juntos.
        let cycle = t / CC_BOLT_PERIOD + hash01(i, 0, 0, self.seed) * 7.0;
        let epoch = cycle.floor() as i32;
        let s = (cycle - cycle.floor()) / CC_BOLT_ACTIVE; // 0..1 mientras está encendido
        if s >= 1.0 {
            return 0.0;
        }
        // Círculo máximo de esta época: su eje y una base (b, c) dentro de su plano.
        let h = |k| hash01(i, epoch, k, self.seed);
        let axis = random_dir(h(1), h(2));
        let b = axis.cross(random_dir(h(3), h(4))).normalized();
        let c = axis.cross(b);
        // La punta avanza de 0 a CC_BOLT_ARC en la primera mitad y luego el rayo se apaga.
        let angle = p.dot(c).atan2(p.dot(b)).rem_euclid(std::f32::consts::TAU);
        let head = (s * 2.0).min(1.0) * CC_BOLT_ARC;
        if angle > head {
            return 0.0;
        }
        // Zigzag: el ruido empuja la distancia al plano, distinto en cada época.
        let jag = fbm(p * CC_BOLT_JAG_FREQ, 2, self.seed.wrapping_add((epoch * 31 + i) as u32)) * CC_BOLT_JAG;
        let d = (p.dot(axis) + jag).abs();
        let core = 1.0 - smoothstep(0.0, CC_BOLT_WIDTH, d);
        let halo = (-d / (CC_BOLT_WIDTH * 3.0)).exp() * 0.35;
        // Se desvanece al final y parpadea rápido como electricidad.
        let fade = 1.0 - smoothstep(0.5, 1.0, s);
        let flicker = 0.75 + 0.25 * (t * 55.0 + i as f32 * 1.7).sin();
        (core + halo) * fade * flicker
    }
}

impl FragmentShader for CentralCityShader {
    fn shade(&self, f: &Fragment, u: &Uniforms) -> Vec3 {
        let p = f.local_pos.normalized(); // gira con la luna
        // Roca: dos tonos mezclados con ruido + Lambert.
        let n = fbm(p * 5.0, 4, self.seed) * 0.5 + 0.5;
        let rock = mix(CC_ROCK_DARK, CC_ROCK_LIGHT, smoothstep(0.35, 0.7, n)) * lambert(f, u, 0.08);
        // Rayos: se SUMAN (emisivos), así brillan también del lado oscuro.
        let glow: f32 = (0..CC_BOLTS).map(|i| self.bolt(i, p, u.time)).sum();
        rock + CC_BOLT_GLOW * glow + CC_BOLT_CORE * (glow * glow * 0.5)
    }
}

// ---------------------------------------------------------------------------
// MODELOS DE SUPERFICIE (S7). Cada uno recibe `height` (alto del modelo en el mundo) para
// saber qué tan arriba está cada píxel: t = local_pos.y / height va de 0 (base) a 1 (punta).
// ---------------------------------------------------------------------------

/// CRISTALES de Krypton: azul emisivo (más claro hacia la punta) con bordes rojos.
/// Los "bordes" son las caras que se ven de lado: (1 − |n·v|)² → como las venas del planeta.
pub struct CrystalPropShader {
    pub height: f32,
}

impl FragmentShader for CrystalPropShader {
    fn shade(&self, f: &Fragment, u: &Uniforms) -> Vec3 {
        let t = (f.local_pos.y / self.height).clamp(0.0, 1.0);
        let l = (u.light_pos - f.world_pos).normalized();
        let v = (u.camera_pos - f.world_pos).normalized();
        let h = (l + v).normalized();
        let diff = f.normal.dot(l).max(0.0);
        let spec = f.normal.dot(h).max(0.0).powf(80.0) * 1.2;
        let base = mix(KRYPTON_DEEP_BLUE, KRYPTON_CYAN, t);
        let glow = KRYPTON_CYAN * (0.2 + 0.35 * t); // luz propia: se ve también de noche
        let edge = (1.0 - f.normal.dot(v).abs()).powf(2.0);
        base * (0.1 + 0.9 * diff) + Vec3::splat(spec) + glow + KRYPTON_VEIN_RED * (edge * 1.3)
    }
}

/// ÁRBOLES de Themyscira: tronco café abajo y en el centro, copa verde olivo con manchas.
const TREE_TRUNK_TOP: f32 = 0.35; // hasta qué fracción del alto puede haber tronco
const TREE_TRUNK_RADIUS: f32 = 0.12; // qué tan lejos del eje (en fracción del alto) es tronco
const TREE_TRUNK: Vec3 = Vec3::new(0.30, 0.20, 0.12);
const TREE_LEAVES_DARK: Vec3 = Vec3::new(0.16, 0.28, 0.10);
const TREE_LEAVES: Vec3 = Vec3::new(0.32, 0.48, 0.18);

pub struct TreePropShader {
    pub height: f32,
}

impl FragmentShader for TreePropShader {
    fn shade(&self, f: &Fragment, u: &Uniforms) -> Vec3 {
        let p = f.local_pos;
        let t = (p.y / self.height).clamp(0.0, 1.0);
        let from_axis = (p.x * p.x + p.z * p.z).sqrt() / self.height;
        let color = if t < TREE_TRUNK_TOP && from_axis < TREE_TRUNK_RADIUS {
            TREE_TRUNK
        } else {
            let leaf = fbm(p * (25.0 / self.height), 2, 5) * 0.5 + 0.5;
            mix(TREE_LEAVES_DARK, TREE_LEAVES, leaf) * (0.85 + 0.3 * t) // la punta recibe más sol
        };
        color * lambert(f, u, 0.08)
    }
}

/// EDIFICIOS de Gotham: concreto con una cuadrícula de ventanas en las fachadas.
/// De día el vidrio es oscuro; de noche (igual que el planeta) algunas ventanas se encienden
/// y cambian cada pocos segundos.
const TOWER_FLOORS: f32 = 10.0; // pisos a lo alto del modelo
const TOWER_COLUMNS: f32 = 14.0; // ventanas a lo ancho (por unidad de alto)

pub struct TowerPropShader {
    pub height: f32,
    pub seed: u32,
}

impl FragmentShader for TowerPropShader {
    fn shade(&self, f: &Fragment, u: &Uniforms) -> Vec3 {
        let p = f.local_pos;
        let l = (u.light_pos - f.world_pos).normalized();
        let diff = f.normal.dot(l).max(0.0);
        let night = 1.0 - smoothstep(GOTHAM_NIGHT - 0.08, GOTHAM_NIGHT + 0.08, diff);
        let light = GOTHAM_AMBIENT + (1.0 - GOTHAM_AMBIENT) * diff;

        // Cuadrícula de ventanas: columnas a lo largo de x + z, pisos a lo largo de y.
        let wx = (p.x + p.z) / self.height * TOWER_COLUMNS;
        let wy = p.y / self.height * TOWER_FLOORS;
        let (cx, cy) = (wx - wx.floor(), wy - wy.floor());
        let top = p.y / self.height > 0.97; // la azotea no lleva ventanas
        let glass = !top && cx > 0.25 && cx < 0.75 && cy > 0.3 && cy < 0.8;
        if !glass {
            return GOTHAM_CONCRETE * light;
        }
        let id = (wx.floor() as i32, wy.floor() as i32);
        let phase = hash01(id.0, id.1, 2, self.seed) * 10.0;
        let epoch = (u.time * GOTHAM_FLICKER_SPEED + phase).floor() as i32;
        let on = hash01(id.0, id.1, epoch, self.seed.wrapping_add(3)) < GOTHAM_WINDOW_DENSITY;
        let emit = if on { GOTHAM_WINDOW * 0.8 } else { Vec3::ZERO };
        GOTHAM_GLASS * light + emit * night
    }
}

/// CORALES de Atlantis: de rosado en la base a naranja en las puntas, con manchas.
const CORAL_PINK: Vec3 = Vec3::new(0.80, 0.42, 0.52);
const CORAL_ORANGE: Vec3 = Vec3::new(0.92, 0.55, 0.30);

pub struct CoralPropShader {
    pub height: f32,
}

impl FragmentShader for CoralPropShader {
    fn shade(&self, f: &Fragment, u: &Uniforms) -> Vec3 {
        let p = f.local_pos;
        let t = (p.y / self.height).clamp(0.0, 1.0);
        let spots = fbm(p * (30.0 / self.height), 2, 11) * 0.3;
        let color = mix(CORAL_PINK, CORAL_ORANGE, t + spots);
        // Un poco de luz "que atraviesa" en los bordes, como un material blando.
        let v = (u.camera_pos - f.world_pos).normalized();
        let rim = (1.0 - f.normal.dot(v).abs()).powf(3.0) * 0.25;
        color * lambert(f, u, 0.12) + CORAL_PINK * rim
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

    /// Color del cielo en la dirección `view_dir` (se llama en cada píxel vacío).
    pub fn color(&self, view_dir: Vec3, u: &Uniforms) -> Vec3 {
        // Agujero negro: cerca de él la luz se dobla (lente gravitacional), así que el resto
        // del cielo (nebulosa y estrellas) se calcula con la dirección YA desviada.
        let hole = black_hole(view_dir, u.time);
        if hole.in_shadow {
            return hole.emission; // dentro de la sombra no llega luz de atrás: solo el disco
        }
        let dir = hole.dir;

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
        let g = view_dir.dot(to_sun).max(0.0);
        c += Vec3::new(1.0, 0.55, 0.15) * (g.powf(60.0) * 0.6 + g.powf(8.0) * 0.08);

        // Disco de acreción y anillo de fotones del agujero negro (cero lejos de él).
        c += hole.emission;

        // Efecto warp (solo cuesta cuando está activo).
        if u.warp > 0.0 {
            c += warp_streaks(view_dir, u) * u.warp;
        }
        c
    }
}

// ---------------------------------------------------------------------------
// AGUJERO NEGRO lejano (Diego, D7): está tan lejos que solo importa su DIRECCIÓN.
// Lente gravitacional con la ecuación de lente:  β = θ − θE² / θ
//   θ  = ángulo entre lo que mira el píxel y el centro del agujero
//   β  = ángulo de donde viene en realidad esa luz (si sale negativo, del otro lado:
//        por eso las estrellas de atrás aparecen "reflejadas" formando un anillo)
//   θE = radio del anillo de Einstein
// Encima: sombra negra, anillo de fotones y un disco de acreción animado e inclinado.
// ---------------------------------------------------------------------------
/// Hacia dónde está el agujero negro (se ve a la derecha y arriba del sol al empezar).
pub const BH_DIR: Vec3 = Vec3::new(0.42, 0.22, -0.88);
/// Radio angular del anillo de Einstein (radianes): qué tan fuerte dobla la luz.
const BH_EINSTEIN: f32 = 0.06;
/// Radio angular de la sombra negra.
const BH_SHADOW: f32 = 0.045;
/// Hasta qué ángulo se nota la lente (después se apaga suave para que no haya un borde).
const BH_INFLUENCE: f32 = 0.5;
/// Disco de acreción: radios interior y exterior (radianes) y qué tan de canto se ve
/// (cos de la inclinación: chico = muy aplastado).
const BH_DISK_INNER: f32 = 0.06;
const BH_DISK_OUTER: f32 = 0.16;
const BH_DISK_COS_TILT: f32 = 0.22;
/// Qué tan rápido gira el disco (los anillos de adentro giran más rápido).
const BH_DISK_SPIN: f32 = 0.08;
const BH_DISK_HOT: Vec3 = Vec3::new(1.0, 0.85, 0.55);
const BH_DISK_COOL: Vec3 = Vec3::new(0.9, 0.35, 0.08);

/// Lo que el agujero negro le hace a un píxel del cielo.
struct BlackHole {
    /// Dirección desviada con la que se sigue calculando el cielo.
    dir: Vec3,
    /// true = el píxel cae en la sombra (no se ve nada de atrás).
    in_shadow: bool,
    /// Luz propia: disco de acreción + anillo de fotones.
    emission: Vec3,
}

fn black_hole(dir: Vec3, time: f32) -> BlackHole {
    let c = BH_DIR.normalized();
    let cos_t = dir.dot(c).clamp(-1.0, 1.0);
    let theta = cos_t.acos();
    if theta > BH_INFLUENCE {
        return BlackHole { dir, in_shadow: false, emission: Vec3::ZERO }; // lejos: sin efecto
    }

    // Base alrededor del centro del agujero: e1 "a la derecha", e2 "hacia arriba".
    let e1 = c.cross(Vec3::UP).normalized();
    let e2 = e1.cross(c);
    let (x, y) = (dir.dot(e1), dir.dot(e2)); // ≈ desplazamiento angular desde el centro

    // --- Lente: β = θ − θE²/θ, apagando la desviación cerca del borde de influencia ---
    let fade = 1.0 - smoothstep(0.5 * BH_INFLUENCE, BH_INFLUENCE, theta);
    let beta = theta - BH_EINSTEIN * BH_EINSTEIN / theta.max(1e-4) * fade;
    let perp = (dir - c * cos_t).normalized(); // dirección "hacia afuera" desde el centro
    let bent = (c * beta.cos() + perp * beta.sin()).normalized();

    // --- Disco de acreción: un anillo inclinado → en pantalla es una elipse aplastada ---
    let rho = (x * x + (y / BH_DISK_COS_TILT).powi(2)).sqrt();
    let mut emission = Vec3::ZERO;
    if rho > BH_DISK_INNER && rho < BH_DISK_OUTER {
        let r01 = (rho - BH_DISK_INNER) / (BH_DISK_OUTER - BH_DISK_INNER);
        // Giro diferencial: el ángulo avanza más rápido en los anillos de adentro.
        let phi = (y / BH_DISK_COS_TILT).atan2(x) - time * BH_DISK_SPIN / rho;
        let swirl = 0.55 + 0.45 * fbm(Vec3::new(phi.cos() * 2.0, phi.sin() * 2.0, rho * 60.0), 3, 808);
        // Efecto Doppler: el lado que viene hacia nosotros (x > 0) brilla más.
        let doppler = 1.0 + 0.6 * (x / rho);
        let edge = smoothstep(0.0, 0.15, r01) * (1.0 - smoothstep(0.6, 1.0, r01));
        // La mitad de atrás del disco (y > 0) queda tapada por la sombra.
        let behind = y > 0.0 && theta < BH_SHADOW;
        if !behind {
            emission += BH_DISK_HOT.lerp(BH_DISK_COOL, r01) * (edge * swirl * doppler * 1.6);
        }
    }

    // --- Anillo de fotones: luz que dio vueltas al agujero, justo afuera de la sombra ---
    let ring = (-((theta - BH_SHADOW * 1.12) / (BH_SHADOW * 0.12)).powi(2)).exp();
    emission += BH_DISK_HOT * (ring * 0.9);

    BlackHole { dir: bent, in_shadow: theta < BH_SHADOW, emission }
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
