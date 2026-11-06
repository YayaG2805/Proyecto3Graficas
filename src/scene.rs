//! Escena: sol y planetas (cuerpos celestes). Aquí se configuran radios, órbitas,
//! velocidades y shaders. Todo movimiento depende del TIEMPO, no de los cuadros.

use crate::math::{Mat4, Vec3};
use crate::mesh::Mesh;
use crate::noise::fbm;
use crate::normal_map::NormalMap;
use crate::props::{
    instance_model, load_prop_lods, scatter, test_pyramid, PropInstance, PropKind, PROPS_DIR, PROP_SEED,
};
use crate::renderer::{FragmentShader, Renderer};
use crate::shaders::{
    gotham_height, themyscira_elevation, AtlantisShader, CentralCityShader, GothamShader, KryptonShader, OaShader,
    OrbitShader, SunShader, ThemysciraShader,
};
use crate::shaders::{
    atlantis_seabed, gotham_cell, CoralPropShader, CrystalPropShader, GothamKind, TowerPropShader, TreePropShader,
    ATLANTIS_REEF_LEVEL,
};
use std::sync::Arc;

/// Cuántos segmentos de línea forman cada órbita (más = círculo más redondo).
const ORBIT_SEGMENTS: usize = 128;

// --- Themyscira (los umbrales de color y el nivel del mar están en shaders.rs) ---
/// Semilla del terreno: cambiarla mueve las islas.
const THEMYSCIRA_SEED: u32 = 7;
/// Qué tanto suben las islas sobre el mar, en fracción del radio (0.06 = 6%).
const THEMYSCIRA_RELIEF: f32 = 0.06;

// --- Krypton (tamaño de cristales, venas y colores están en shaders.rs) ---
/// Semilla de las crestas y de los cristales.
const KRYPTON_SEED: u32 = 21;
/// Altura de las crestas de cristal, en fracción del radio (0.08 = 8%).
const KRYPTON_RELIEF: f32 = 0.08;
/// Frecuencia de las cordilleras (más alto = más crestas y más juntas).
const KRYPTON_RIDGE_FREQ: f32 = 2.5;
/// Qué tan afiladas son las crestas (exponente: más alto = más finas y puntiagudas).
const KRYPTON_RIDGE_SHARPNESS: f32 = 3.0;

/// Ruido RIDGED para Krypton: 1 - |fbm| vale 1 justo donde el ruido cruza por 0, así que
/// se forman "líneas" de crestas afiladas en vez de colinas redondas.
fn krypton_ridge(dir: Vec3) -> f32 {
    let ridge = (1.0 - fbm(dir * KRYPTON_RIDGE_FREQ, 4, KRYPTON_SEED).abs() * 2.0).max(0.0);
    ridge.powf(KRYPTON_RIDGE_SHARPNESS)
}

// --- Gotham (manzanas, ventanas y colores están en shaders.rs) ---
/// Semilla de la ciudad: cambiarla cambia alturas, tonos y ventanas.
const GOTHAM_SEED: u32 = 33;
/// Altura de la torre más alta, en fracción del radio (0.035 = 3.5%).
const GOTHAM_BUILDING_HEIGHT: f32 = 0.035;

// --- Atlantis (olas, arrecifes y colores están en shaders.rs: WAVE_SPEED, WAVE_HEIGHT, WAVE_FREQ) ---
/// Semilla del fondo marino y de los arrecifes.
const ATLANTIS_SEED: u32 = 45;

// --- Oa (anillos, circuitos y colores están en shaders.rs) ---
/// Semilla de los circuitos y del relieve.
const OA_SEED: u32 = 57;
/// Relieve muy leve (fracción del radio): casi todo el efecto es del shader.
const OA_RELIEF: f32 = 0.015;

// --- Central City, luna de Gotham (rayos y colores están en shaders.rs) — Diego, D4 ---
/// Radio de la luna.
const CENTRAL_CITY_RADIUS: f32 = 1.2;
/// Distancia al centro de Gotham.
const CENTRAL_CITY_ORBIT: f32 = 9.0;
/// Radianes por segundo alrededor de Gotham (mucho más rápida que los planetas).
const CENTRAL_CITY_ORBIT_SPEED: f32 = 0.6;
/// Semilla de los cráteres y de los rayos.
const CENTRAL_CITY_SEED: u32 = 69;
/// Relieve de los cráteres, en fracción del radio.
const CENTRAL_CITY_RELIEF: f32 = 0.05;

// --- Objetos de superficie (S6 + S7). La semilla PROP_SEED está en props.rs ---
// Cada modelo se carga de assets/props/<archivo>.obj; si todavía no existe, se usa una pirámide.
// Alto de cada modelo en el mundo (el .obj se escala a esta medida) y cuántos hay.
const CRYSTAL_HEIGHT: f32 = 0.55;
const CRYSTAL_COUNT: usize = 60;
const TREE_HEIGHT: f32 = 0.5;
const TREE_COUNT: usize = 60;
const TOWER_HEIGHT: f32 = 0.45;
const TOWER_COUNT: usize = 120;
const CORAL_HEIGHT: f32 = 0.3;
const CORAL_COUNT: usize = 80;
/// En Themyscira los árboles van en el pasto: sobre la playa y debajo de la piedra.
const THEMYSCIRA_PROP_MIN_ELEVATION: f32 = 0.03;
const THEMYSCIRA_PROP_MAX_ELEVATION: f32 = 0.15;

/// la ficha de un mundo, lo q se ve en el hud cuando lo escaneas
pub struct PlanetInfo {
    pub title: &'static str,
    pub hero: &'static str,
    /// dato curioso pal escaneo
    pub fact: &'static str,
}

pub struct Body {
    pub name: &'static str,
    /// None = orbita al sol, Some(i) = es luna del cuerpo i
    pub parent: Option<usize>,
    /// fuerza con la q atrae a la nave (a = gravity / d²)
    pub gravity: f32,
    pub info: PlanetInfo,
    /// objetos parados sobre la superficie, los llena sando en s6
    pub props: Vec<PropInstance>,
    pub radius: f32,
    /// Distancia a lo que orbita: el sol, o su planeta si es luna (0 = el sol).
    pub orbit_radius: f32,
    /// Radianes por segundo alrededor de lo que orbita.
    pub orbit_speed: f32,
    /// Ángulo inicial en la órbita.
    pub orbit_phase: f32,
    /// Radianes por segundo sobre su propio eje.
    pub spin_speed: f32,
    /// Inclinación del eje (radianes).
    pub tilt: f32,
    /// Forma de la superficie: radio relativo (1 = esfera) en cada dirección. Es la MISMA
    /// función que deforma la malla, así los objetos de superficie quedan sobre el terreno.
    pub surface: fn(Vec3) -> f32,
    /// Niveles de detalle: [0] = más detallado. Se elige según la distancia.
    pub lods: Vec<Mesh>,
    pub shader: Arc<dyn FragmentShader>,
}

impl Body {
    /// donde va en su orbita, medido desde lo q orbita (circulo en el plano xz)
    /// ojo: pa la posicion real en el mundo hay q usar Scene::body_position
    fn orbit_offset(&self, t: f32) -> Vec3 {
        self.orbit_point(self.orbit_phase + self.orbit_speed * t)
    }

    /// Punto de la órbita en el ángulo `a` (radianes).
    fn orbit_point(&self, a: f32) -> Vec3 {
        Vec3::new(a.cos() * self.orbit_radius, 0.0, a.sin() * self.orbit_radius)
    }

    /// La órbita completa como puntos: la misma fórmula de `orbit_offset`, dando toda la vuelta.
    pub fn orbit_points(&self) -> Vec<Vec3> {
        (0..ORBIT_SEGMENTS)
            .map(|i| self.orbit_point(i as f32 / ORBIT_SEGMENTS as f32 * std::f32::consts::TAU))
            .collect()
    }
}

// --- Formas de la superficie de cada cuerpo (radio relativo en cada dirección) ---
// Se usan para deformar la malla Y para saber a qué altura poner los objetos de superficie.
fn themyscira_surface(d: Vec3) -> f32 {
    1.0 + themyscira_elevation(d, THEMYSCIRA_SEED).max(0.0) * THEMYSCIRA_RELIEF
}
fn krypton_surface(d: Vec3) -> f32 {
    1.0 + krypton_ridge(d) * KRYPTON_RELIEF
}
fn gotham_surface(d: Vec3) -> f32 {
    1.0 + gotham_height(d, GOTHAM_SEED) * GOTHAM_BUILDING_HEIGHT
}
fn oa_surface(d: Vec3) -> f32 {
    1.0 + fbm(d * 4.0, 3, OA_SEED) * OA_RELIEF
}
fn central_city_surface(d: Vec3) -> f32 {
    1.0 + fbm(d * 3.0, 3, CENTRAL_CITY_SEED) * CENTRAL_CITY_RELIEF
}

/// Genera las mallas de LOD de una esfera (opcionalmente deformada).
/// subdivisiones 5,4,3,2 → 20480, 5120, 1280, 320 triángulos.
fn sphere_lods(displace: &dyn Fn(Vec3) -> f32) -> Vec<Mesh> {
    [5, 4, 3, 2].iter().map(|&s| Mesh::icosphere(s, displace)).collect()
}

pub struct Scene {
    pub bodies: Vec<Body>,
    /// los tipos de objeto de superficie, PropInstance.kind es un indice de esta lista
    pub prop_kinds: Vec<PropKind>,
    /// Dibujar las órbitas (tecla O).
    pub show_orbits: bool,
    pub orbit_shader: Arc<dyn FragmentShader>,
}

impl Scene {
    pub fn new() -> Scene {
        let sun = Body {
            name: "Sol",
            parent: None,
            gravity: 800.0,
            info: PlanetInfo {
                title: "Sol",
                hero: "Sin héroe",
                fact: "Es la única fuente de luz del sistema y lo que más atrae a la nave.",
            },
            props: Vec::new(),
            radius: 12.0,
            orbit_radius: 0.0,
            orbit_speed: 0.0,
            orbit_phase: 0.0,
            spin_speed: 0.05,
            tilt: 0.0,
            surface: |_| 1.0,
            lods: sphere_lods(&|_| 1.0),
            shader: Arc::new(SunShader { seed: 1, speed: 0.15 }),
        };

        // THEMYSCIRA (Mujer Maravilla): islas de piedra clara y pasto sobre un mar Mediterráneo.
        // Deformación con AGUA PLANA: radio = 1 + max(0, elevación) × relieve.
        // Donde la elevación es negativa (mar) el radio queda en 1 exacto → superficie lisa.
        let themyscira = Body {
            name: "Themyscira",
            parent: None,
            gravity: 60.0,
            info: PlanetInfo {
                title: "Themyscira",
                hero: "Mujer Maravilla",
                fact: "Isla oculta de las amazonas; ningún hombre podía pisarla.",
            },
            props: Vec::new(),
            radius: 4.0,
            orbit_radius: 45.0,
            orbit_speed: 0.08,
            orbit_phase: 0.0,
            spin_speed: 0.25,
            tilt: 0.3,
            surface: themyscira_surface,
            lods: sphere_lods(&themyscira_surface),
            shader: Arc::new(ThemysciraShader {
                seed: THEMYSCIRA_SEED,
                // Mapa de normales 512x256: frecuencia 10 (tamaño de las rocas), relieve 3% del radio.
                normal_map: Some(NormalMap::generate(512, 256, THEMYSCIRA_SEED + 100, 10.0, 0.03)),
                normal_strength: 1.0,
            }),
        };

        // KRYPTON (Superman): cristales azules con venas rojas brillantes.
        // Deformación RIDGED: radio = 1 + relieve × (1 - |fbm|)^afilado → cordilleras de cristal.
        let krypton = Body {
            name: "Krypton",
            parent: None,
            gravity: 90.0,
            info: PlanetInfo {
                title: "Krypton",
                hero: "Superman",
                fact: "Su sol rojo no le daba poderes a Kal-El; el sol amarillo de la Tierra sí.",
            },
            props: Vec::new(),
            radius: 5.0,
            orbit_radius: 70.0,
            orbit_speed: 0.05,
            orbit_phase: 2.4, // distinta a Themyscira para que no queden alineados
            spin_speed: 0.18,
            tilt: 0.15,
            surface: krypton_surface,
            lods: sphere_lods(&krypton_surface),
            shader: Arc::new(KryptonShader { seed: KRYPTON_SEED }),
        };

        // GOTHAM (Batman): planeta-ciudad oscuro, casi siempre de noche.
        // Deformación EN ESCALÓN: cada manzana sube a su propia altura, como edificios.
        let gotham = Body {
            name: "Gotham",
            parent: None,
            gravity: 100.0,
            info: PlanetInfo {
                title: "Gotham",
                hero: "Batman",
                fact: "Bruce Wayne patrulla de noche; de día es el dueño de Industrias Wayne.",
            },
            props: Vec::new(),
            radius: 5.5,
            orbit_radius: 100.0,
            orbit_speed: 0.035,
            orbit_phase: 4.4, // distinta a los demás para que no queden alineados
            spin_speed: 0.12,
            tilt: 0.2,
            surface: gotham_surface,
            lods: sphere_lods(&gotham_surface),
            shader: Arc::new(GothamShader { seed: GOTHAM_SEED }),
        };

        // ATLANTIS (Aquaman): océano con olas en movimiento y arrecifes.
        // SIN deformación: la superficie del mar es lisa; las olas están en el shader (en la normal).
        let atlantis = Body {
            name: "Atlantis",
            parent: None,
            gravity: 110.0,
            info: PlanetInfo {
                title: "Atlantis",
                hero: "Aquaman",
                fact: "Arthur Curry puede hablar con la vida marina y es rey de los siete mares.",
            },
            props: Vec::new(),
            radius: 6.0,
            orbit_radius: 130.0,
            orbit_speed: 0.028,
            orbit_phase: 0.9, // distinta a los demás para que no queden alineados
            spin_speed: 0.1,
            tilt: 0.25,
            surface: |_| 1.0,
            lods: sphere_lods(&|_| 1.0),
            shader: Arc::new(AtlantisShader { seed: ATLANTIS_SEED }),
        };

        // OA (Linterna Verde): planeta verde que pulsa con energía.
        // Deformación muy leve con ruido para que no sea una esfera perfecta.
        let oa = Body {
            name: "Oa",
            parent: None,
            gravity: 70.0,
            info: PlanetInfo {
                title: "Oa",
                hero: "Linterna Verde",
                fact: "Es el centro del universo y la base de los Guardianes y del Cuerpo de Linternas Verdes.",
            },
            props: Vec::new(),
            radius: 4.5,
            orbit_radius: 160.0,
            orbit_speed: 0.022,
            orbit_phase: 3.3, // distinta a los demás para que no queden alineados
            spin_speed: 0.3,
            tilt: 0.1,
            surface: oa_surface,
            lods: sphere_lods(&oa_surface),
            shader: Arc::new(OaShader { seed: OA_SEED }),
        };

        let mut bodies = vec![sun, themyscira, krypton, gotham, atlantis, oa];

        // CENTRAL CITY (Flash): luna de Gotham. JERARQUÍA DE TRANSFORMACIONES:
        // su posición = posición de Gotham + su propia órbita de radio 9 alrededor de él,
        // así que acompaña a Gotham mientras éste orbita al sol (ver body_position).
        // Va al final de la lista para no cambiar los números 1-5 de los mundos.
        let gotham_index = bodies.iter().position(|b| b.name == "Gotham").expect("falta Gotham");
        bodies.push(Body {
            name: "Central City",
            parent: Some(gotham_index),
            gravity: 25.0,
            info: PlanetInfo {
                title: "Central City",
                hero: "Flash",
                fact: "Barry Allen obtuvo su supervelocidad cuando un rayo cayó sobre los químicos de su laboratorio.",
            },
            props: Vec::new(),
            radius: CENTRAL_CITY_RADIUS,
            orbit_radius: CENTRAL_CITY_ORBIT,
            orbit_speed: CENTRAL_CITY_ORBIT_SPEED,
            orbit_phase: 0.0,
            spin_speed: 0.4,
            tilt: 0.0,
            surface: central_city_surface,
            lods: sphere_lods(&central_city_surface),
            shader: Arc::new(CentralCityShader { seed: CENTRAL_CITY_SEED }),
        });

        // OBJETOS DE SUPERFICIE (S6 + S7): un tipo de modelo por mundo, repartido con scatter.
        // Cada planeta usa una semilla derivada de PROP_SEED para no repetir el mismo patrón.
        let pyramid = test_pyramid();
        let mut prop_kinds: Vec<PropKind> = Vec::new();
        // Carga el modelo de Blender (con su LOD); si todavía no existe, usa la pirámide de prueba.
        let mut add_kind = |name: &'static str, file: &str, height: f32, shader: Arc<dyn FragmentShader>| {
            let lods = load_prop_lods(file, height).unwrap_or_else(|| {
                eprintln!("Falta {PROPS_DIR}/{file}.obj: se usa la pirámide de prueba");
                vec![pyramid.clone(), pyramid.clone()]
            });
            prop_kinds.push(PropKind { name, lods, shader });
            prop_kinds.len() - 1 // su índice en prop_kinds
        };
        let crystal = add_kind("Cristales", "cristal", CRYSTAL_HEIGHT, Arc::new(CrystalPropShader { height: CRYSTAL_HEIGHT }));
        let tree = add_kind("Árboles", "arbol", TREE_HEIGHT, Arc::new(TreePropShader { height: TREE_HEIGHT }));
        let tower = add_kind(
            "Edificios",
            "edificio",
            TOWER_HEIGHT,
            Arc::new(TowerPropShader { height: TOWER_HEIGHT, seed: GOTHAM_SEED }),
        );
        let coral = add_kind("Corales", "coral", CORAL_HEIGHT, Arc::new(CoralPropShader { height: CORAL_HEIGHT }));

        let index_of = |name: &str| bodies.iter().position(|b| b.name == name).expect("falta un mundo");
        let (themyscira_i, krypton_i) = (index_of("Themyscira"), index_of("Krypton"));
        let (gotham_i, atlantis_i) = (index_of("Gotham"), index_of("Atlantis"));
        // Krypton: cristales en cualquier parte.
        bodies[krypton_i].props = scatter(PROP_SEED.wrapping_add(1), CRYSTAL_COUNT, crystal, &|_| true);
        // Themyscira: árboles solo en el pasto (ni en el mar, ni en la playa, ni en la piedra).
        bodies[themyscira_i].props = scatter(PROP_SEED, TREE_COUNT, tree, &|d| {
            let e = themyscira_elevation(d, THEMYSCIRA_SEED);
            e > THEMYSCIRA_PROP_MIN_ELEVATION && e < THEMYSCIRA_PROP_MAX_ELEVATION
        });
        // Gotham: edificios solo sobre azoteas (no en calles, carreteras, parques ni agua).
        bodies[gotham_i].props = scatter(PROP_SEED.wrapping_add(2), TOWER_COUNT, tower, &|d| {
            gotham_cell(d, GOTHAM_SEED).kind == GothamKind::Roof
        });
        // Atlantis: corales solo donde el fondo marino sube (las mismas zonas de arrecife del shader).
        bodies[atlantis_i].props = scatter(PROP_SEED.wrapping_add(3), CORAL_COUNT, coral, &|d| {
            atlantis_seabed(d, ATLANTIS_SEED) > ATLANTIS_REEF_LEVEL
        });

        Scene {
            bodies,
            prop_kinds,
            show_orbits: true,
            orbit_shader: Arc::new(OrbitShader { color: Vec3::new(0.95, 0.55, 0.3), dash_length: 3.0, speed: 3.0 }),
        }
    }

    /// donde esta de verdad el cuerpo i en el mundo, siempre hay q usar esta
    pub fn body_position(&self, i: usize, t: f32) -> Vec3 {
        let b = &self.bodies[i];
        // si es luna su orbita se suma a donde va su planeta, si no se mide desde el sol (el origen)
        match b.parent {
            Some(p) => self.body_position(p, t) + b.orbit_offset(t),
            None => b.orbit_offset(t),
        }
    }

    /// el marco del cuerpo: posicion + inclinacion + giro, pero SIN escala
    /// sirve pa pegarle cosas encima sin q se estiren con el radio del planeta
    pub fn body_frame(&self, i: usize, t: f32) -> Mat4 {
        let b = &self.bodies[i];
        Mat4::translation(self.body_position(i, t)) * Mat4::rotation_z(b.tilt) * Mat4::rotation_y(b.spin_speed * t)
    }

    /// la matriz de modelo completa: el marco de arriba * escala(radio)
    pub fn model_matrix(&self, i: usize, t: f32) -> Mat4 {
        self.body_frame(i, t) * Mat4::scale(Vec3::splat(self.bodies[i].radius))
    }

    pub fn draw(&self, r: &mut Renderer, t: f32) {
        for (i, b) in self.bodies.iter().enumerate() {
            // primero el cuerpo, el renderer elige solito el lod segun q tan grande se ve
            r.draw_lods(&b.lods, &self.model_matrix(i, t), b.shader.clone());

            // despues lo q tenga parado encima, todos usan el mismo marco del planeta
            let frame = self.body_frame(i, t);
            for p in &b.props {
                let kind = &self.prop_kinds[p.kind];
                // radio de la superficie en esa dirección (con montañas, crestas o edificios)
                let model = instance_model(&frame, b.radius * (b.surface)(p.dir), p);
                r.draw_lods(&kind.lods, &model, kind.shader.clone());
            }
        }
        if self.show_orbits {
            // El sol no orbita (radio 0), así que no tiene línea.
            for b in self.bodies.iter().filter(|b| b.orbit_radius > 0.0) {
                // las orbitas salen medidas desde el centro, asi q las movemos a donde va el padre
                // pa los planetas el padre es el sol en el origen y no cambia nada, pero las lunas si
                let center = b.parent.map_or(Vec3::ZERO, |p| self.body_position(p, t));
                let points: Vec<Vec3> = b.orbit_points().into_iter().map(|p| p + center).collect();
                r.draw_line_loop(&points, self.orbit_shader.clone());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn una_luna_se_mueve_junto_con_su_planeta() {
        let mut scene = Scene::new();
        let t = 12.0;

        // convierto a krypton (2) en luna de themyscira (1) solo pa esta prueba
        let alone = scene.body_position(2, t);
        scene.bodies[2].parent = Some(1);

        // su orbita ahora se tiene q sumar a donde va themyscira, con cualquier t
        let expected = scene.body_position(1, t) + alone;
        assert!((scene.body_position(2, t) - expected).length() < 1e-4);
    }

    #[test]
    fn el_marco_del_planeta_no_trae_la_escala() {
        let scene = Scene::new();
        let frame = scene.body_frame(1, 3.0);
        let model = scene.model_matrix(1, 3.0);

        // un vector de largo 1 sigue midiendo 1 con el marco, y mide el radio con la matriz completa
        let up = Vec3::new(0.0, 1.0, 0.0);
        assert!((frame.transform_dir(up).length() - 1.0).abs() < 1e-4);
        assert!((model.transform_dir(up).length() - scene.bodies[1].radius).abs() < 1e-4);
    }
}
