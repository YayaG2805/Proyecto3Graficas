//! OBJETOS SOBRE LA SUPERFICIE de los planetas (rocas, edificios, templos...) — Sando, S6.
//!
//! - `scatter`: decide DÓNDE van, con hash + ruido y una semilla fija (misma semilla = mismas
//!   posiciones). Las instancias se reparten solas sobre el planeta, no se escriben una por una.
//! - `instance_model`: arma la matriz que deja cada objeto PARADO sobre la superficie curva
//!   ("arriba" = la normal del planeta en ese punto) y que gira junto con el planeta.

use crate::math::{Mat4, Vec3};
use crate::mesh::{flat_mesh, Mesh};
use crate::noise::{fbm, hash01};
use crate::renderer::FragmentShader;
use std::f32::consts::TAU;
use std::sync::Arc;

/// Semilla de la distribución. El profe puede pedir cambiarla en vivo: con otra semilla los
/// objetos cambian de lugar; con la misma, quedan exactamente donde estaban.
pub const PROP_SEED: u32 = 2026;
/// Frecuencia del ruido que agrupa los objetos (más alto = grupos más pequeños y numerosos).
const PROP_NOISE_FREQ: f32 = 2.0;
/// Solo se aceptan lugares donde el ruido supera este valor (más alto = menos zonas con objetos).
const PROP_NOISE_THRESHOLD: f32 = 0.0;
/// Cuántos candidatos se prueban como máximo por cada objeto pedido (para no quedarse en un ciclo).
const PROP_MAX_TRIES: usize = 40;
/// Rango de tamaños: cada instancia sale con una escala al azar entre estos dos.
const PROP_SCALE_MIN: f32 = 0.7;
const PROP_SCALE_MAX: f32 = 1.3;

/// un tipo de objeto: su modelo en varios niveles de detalle y su shader
pub struct PropKind {
    // el allow se quita cuando el hud o la s7 muestren el nombre
    #[allow(dead_code)]
    pub name: &'static str,
    pub lods: Vec<Mesh>,
    pub shader: Arc<dyn FragmentShader>,
}

/// un objeto puesto en un planeta, muchos pueden compartir el mismo tipo
pub struct PropInstance {
    /// indice en Scene::prop_kinds
    pub kind: usize,
    /// direccion unitaria desde el centro del planeta (espacio local)
    pub dir: Vec3,
    pub scale: f32,
    /// giro sobre su propio eje "arriba"
    pub yaw: f32,
}

/// Reparte `count` instancias con ruido y semilla fija. Misma semilla = mismas posiciones.
///
/// 1. El candidato número i saca dos números con `hash01(i, semilla)` y los convierte en una
///    dirección UNIFORME sobre la esfera: y = 2·h₁ − 1 (altura), ángulo = 2π·h₂ (vuelta).
/// 2. Se acepta solo si el fBm en esa dirección supera el umbral (así salen GRUPOS, como
///    bosques o campos de cristales) y si `accept(dir)` dice que sí (por ejemplo: solo en tierra).
/// 3. La escala y el giro también salen del hash, así que también se repiten con la semilla.
pub fn scatter(seed: u32, count: usize, kind: usize, accept: &dyn Fn(Vec3) -> bool) -> Vec<PropInstance> {
    let mut out = Vec::with_capacity(count);
    for i in 0..(count * PROP_MAX_TRIES) as i32 {
        if out.len() == count {
            break;
        }
        let y = 2.0 * hash01(i, 0, 0, seed) - 1.0;
        let ring = (1.0 - y * y).max(0.0).sqrt(); // radio del círculo de latitud a esa altura
        let angle = TAU * hash01(i, 1, 0, seed);
        let dir = Vec3::new(ring * angle.cos(), y, ring * angle.sin());

        if fbm(dir * PROP_NOISE_FREQ, 3, seed) <= PROP_NOISE_THRESHOLD || !accept(dir) {
            continue;
        }
        out.push(PropInstance {
            kind,
            dir,
            scale: PROP_SCALE_MIN + (PROP_SCALE_MAX - PROP_SCALE_MIN) * hash01(i, 2, 0, seed),
            yaw: TAU * hash01(i, 3, 0, seed),
        });
    }
    out
}

/// Matriz de modelo de una instancia: parada sobre la superficie, "arriba" = normal del planeta.
///
/// 1. arriba = dir (en una esfera, la normal es la dirección desde el centro).
/// 2. Una tangente perpendicular: cross(arriba, eje). El eje es Y, salvo que `dir` casi sea Y
///    (en los polos el producto cruz daría casi cero), entonces se usa X.
/// 3. Se gira esa tangente `yaw` alrededor de "arriba" y el "adelante" sale de otro producto cruz.
/// 4. Posición = dir × surface_radius (sobre la superficie). Todo se multiplica por `body_frame`
///    (posición + inclinación + giro del planeta), así el objeto orbita y gira con él.
pub fn instance_model(body_frame: &Mat4, surface_radius: f32, p: &PropInstance) -> Mat4 {
    let up = p.dir.normalized();
    let axis = if up.y.abs() < 0.99 { Vec3::UP } else { Vec3::new(1.0, 0.0, 0.0) };
    let tangent = up.cross(axis).normalized();
    let bitangent = up.cross(tangent);
    let (s, c) = p.yaw.sin_cos();
    let right = tangent * c + bitangent * s;
    // from_basis pone -forward en la columna Z; con forward = up × right la base queda de mano derecha.
    let forward = up.cross(right);
    *body_frame * Mat4::from_basis(right, up, forward, up * surface_radius) * Mat4::scale(Vec3::splat(p.scale))
}

/// Carpeta de los modelos de superficie hechos en Blender (S7).
pub const PROPS_DIR: &str = "assets/props";

/// Carga un modelo de superficie: `assets/props/<name>.obj` como LOD 0 y, si existe,
/// `assets/props/<name>_lod1.obj` (la versión con Decimate) como LOD 1.
/// Cada malla se deja con la BASE EN EL ORIGEN y se escala para que mida `height` de alto,
/// así no importa en qué escala o posición se exportó desde Blender.
/// Devuelve None si falta el archivo principal (entonces la escena usa la pirámide de prueba).
pub fn load_prop_lods(name: &str, height: f32) -> Option<Vec<Mesh>> {
    let main = load_grounded(&format!("{PROPS_DIR}/{name}.obj"), height)?;
    let lod1 = load_grounded(&format!("{PROPS_DIR}/{name}_lod1.obj"), height).unwrap_or_else(|| main.clone());
    Some(vec![main, lod1])
}

/// Lee un .obj y lo "aterriza": x y z centrados, la parte más baja en y = 0, alto = `height`.
fn load_grounded(path: &str, height: f32) -> Option<Mesh> {
    let mut mesh = Mesh::load_obj(path, None).ok()?;
    if mesh.vertices.is_empty() {
        return None;
    }
    let first = mesh.vertices[0].position;
    let (lo, hi) = mesh.vertices.iter().fold((first, first), |(lo, hi), v| {
        let p = v.position;
        (Vec3::new(lo.x.min(p.x), lo.y.min(p.y), lo.z.min(p.z)), Vec3::new(hi.x.max(p.x), hi.y.max(p.y), hi.z.max(p.z)))
    });
    // Punto de apoyo: centro en x/z y lo más bajo en y. Ese punto pasa a ser el origen.
    let base = Vec3::new((lo.x + hi.x) * 0.5, lo.y, (lo.z + hi.z) * 0.5);
    let s = height / (hi.y - lo.y).max(1e-6);
    for v in &mut mesh.vertices {
        v.position = (v.position - base) * s; // escala uniforme: las normales no cambian
    }
    mesh.compute_bounds();
    Some(mesh)
}

/// Pirámide de prueba (base cuadrada en el origen, punta hacia +Y) mientras llegan los modelos
/// de Blender de la S7. Medidas en unidades del mundo (no se escala con el planeta).
pub fn test_pyramid() -> Mesh {
    let (half, height) = (0.12, 0.35);
    let p = [
        Vec3::new(-half, 0.0, -half),
        Vec3::new(half, 0.0, -half),
        Vec3::new(half, 0.0, half),
        Vec3::new(-half, 0.0, half),
        Vec3::new(0.0, height, 0.0),
    ];
    flat_mesh(&p, &[[0, 1, 4], [1, 2, 4], [2, 3, 4], [3, 0, 4], [0, 2, 1], [0, 3, 2]])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn la_misma_semilla_da_las_mismas_posiciones() {
        let a = scatter(PROP_SEED, 30, 0, &|_| true);
        let b = scatter(PROP_SEED, 30, 0, &|_| true);
        assert_eq!(a.len(), 30);
        for (p, q) in a.iter().zip(&b) {
            assert!((p.dir - q.dir).length() < 1e-6);
            assert_eq!(p.scale, q.scale);
            assert_eq!(p.yaw, q.yaw);
        }
    }

    #[test]
    fn otra_semilla_mueve_los_objetos() {
        let a = scatter(PROP_SEED, 30, 0, &|_| true);
        let b = scatter(PROP_SEED + 1, 30, 0, &|_| true);
        let iguales = a.iter().zip(&b).filter(|(p, q)| (p.dir - q.dir).length() < 1e-6).count();
        assert!(iguales < 3);
    }

    #[test]
    fn accept_filtra_y_las_direcciones_son_unitarias() {
        // solo el hemisferio norte
        let norte = scatter(PROP_SEED, 20, 0, &|d| d.y > 0.0);
        assert!(!norte.is_empty());
        for p in &norte {
            assert!(p.dir.y > 0.0);
            assert!((p.dir.length() - 1.0).abs() < 1e-4);
        }
    }

    #[test]
    fn el_objeto_queda_parado_sobre_la_superficie() {
        let frame = Mat4::translation(Vec3::new(10.0, 0.0, 0.0));
        for p in scatter(PROP_SEED, 15, 0, &|_| true) {
            let m = instance_model(&frame, 4.0, &p);
            // su base (origen local) queda a 4 unidades del centro del planeta, en la dirección dir
            let base = m.transform_point(Vec3::ZERO);
            assert!((base - (Vec3::new(10.0, 0.0, 0.0) + p.dir * 4.0)).length() < 1e-3);
            // y su "arriba" local apunta igual que la normal del planeta
            let up = m.transform_dir(Vec3::UP).normalized();
            assert!(up.dot(p.dir) > 0.999);
        }
    }

    #[test]
    fn un_modelo_cargado_queda_con_la_base_en_el_origen_y_el_alto_pedido() {
        // un .obj chiquito, movido y en otra escala, como si viniera así de Blender
        let dir = std::env::temp_dir().join("prop_test_s7");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("caja.obj");
        std::fs::write(
            &path,
            "v 5 2 5\nv 7 2 5\nv 7 2 7\nv 5 2 7\nv 6 6 6\nf 1 2 5\nf 2 3 5\nf 3 4 5\nf 4 1 5\nf 1 3 2\nf 1 4 3\n",
        )
        .unwrap();
        let m = load_grounded(path.to_str().unwrap(), 0.5).expect("debería cargar");
        let min_y = m.vertices.iter().map(|v| v.position.y).fold(f32::MAX, f32::min);
        let max_y = m.vertices.iter().map(|v| v.position.y).fold(f32::MIN, f32::max);
        assert!(min_y.abs() < 1e-4, "la base debe quedar en y = 0");
        assert!((max_y - 0.5).abs() < 1e-4, "debe medir 0.5 de alto");
        // la punta (centro en x/z) queda sobre el eje Y
        let top = m.vertices.iter().find(|v| (v.position.y - 0.5).abs() < 1e-4).unwrap().position;
        assert!(top.x.abs() < 1e-4 && top.z.abs() < 1e-4);
        assert!(load_grounded("assets/props/no_existe.obj", 1.0).is_none());
    }

    #[test]
    fn funciona_en_los_polos() {
        let polo = PropInstance { kind: 0, dir: Vec3::UP, scale: 1.0, yaw: 0.7 };
        let m = instance_model(&Mat4::translation(Vec3::ZERO), 5.0, &polo);
        let up = m.transform_dir(Vec3::UP).normalized();
        assert!(up.dot(Vec3::UP) > 0.999);
    }
}
