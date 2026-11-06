//! objetos parados sobre la superficie de los planetas (rocas, edificios, templos...)
//!
//! por ahora solo estan las estructuras y funciones provisionales q necesita la escena.
//! sando las implementa de verdad en s6, sin cambiar las firmas.

use crate::math::{Mat4, Vec3};
use crate::mesh::Mesh;
use crate::renderer::FragmentShader;
use std::sync::Arc;

/// un tipo de objeto: su modelo en varios niveles de detalle y su shader
// el allow se quita cuando s7 cree el primer tipo de verdad
#[allow(dead_code)]
pub struct PropKind {
    pub name: &'static str,
    pub lods: Vec<Mesh>,
    pub shader: Arc<dyn FragmentShader>,
}

/// un objeto puesto en un planeta, muchos pueden compartir el mismo tipo
// el allow se quita cuando scatter ya cree instancias
#[allow(dead_code)]
pub struct PropInstance {
    /// indice en Scene::prop_kinds
    pub kind: usize,
    /// direccion unitaria desde el centro del planeta (espacio local)
    pub dir: Vec3,
    pub scale: f32,
    /// giro sobre su propio eje "arriba"
    pub yaw: f32,
}

/// reparte `count` instancias con ruido y semilla fija, misma semilla = mismas posiciones
// TODO(Sando): implementarla en s6, por ahora no reparte nada
#[allow(dead_code)]
pub fn scatter(_seed: u32, _count: usize, _kind: usize, _accept: &dyn Fn(Vec3) -> bool) -> Vec<PropInstance> {
    vec![]
}

/// matriz de modelo de una instancia: parada sobre la superficie, "arriba" = normal del planeta
// TODO(Sando): falta orientarla con dir y yaw en s6, por ahora solo la coloca en su lugar
pub fn instance_model(body_frame: &Mat4, surface_radius: f32, p: &PropInstance) -> Mat4 {
    *body_frame * Mat4::translation(p.dir * surface_radius) * Mat4::scale(Vec3::splat(p.scale))
}
