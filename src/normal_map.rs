//! MAPEO DE NORMALES (normal mapping) hecho por nosotros.
//!
//! Idea: la geometría del planeta es "lisa" a escala fina, pero la iluminación depende de la
//! NORMAL. Si cambiamos la normal en cada píxel según un mapa, la luz dibuja grietas y rocas
//! que no existen en la malla (no cuesta triángulos extra).
//!
//! 1. `NormalMap::generate`: al arrancar se genera una imagen de alturas con ruido y de ahí
//!    las normales en ESPACIO TANGENTE (x = hacia +u, y = hacia +v, z = hacia afuera).
//! 2. `NormalMap::apply`: en el fragment shader se arma la base TBN (tangente, bitangente,
//!    normal) del píxel y se pasa la normal del mapa al mundo: n = T*x + B*y + N*z.

use crate::math::Vec3;
use crate::mesh::sphere_uv;
use crate::noise::{fbm, perlin3};
use crate::renderer::Fragment;
use std::f32::consts::PI;

pub struct NormalMap {
    width: usize,
    height: usize,
    /// Normales en espacio tangente (ya normalizadas), una por texel.
    data: Vec<Vec3>,
}

impl NormalMap {
    /// Genera el mapa de un planeta.
    /// - `frequency`: tamaño de los detalles (más grande = rocas más pequeñas y numerosas).
    /// - `height_scale`: qué tan profundo es el relieve, en fracción del radio (0.03 = 3%).
    pub fn generate(width: usize, height: usize, seed: u32, frequency: f32, height_scale: f32) -> NormalMap {
        // --- Paso 1: mapa de alturas. Cada texel (u, v) es un punto de la esfera. ---
        // El ruido se evalúa en 3D sobre la esfera, así no hay costura donde u vuelve a 0.
        let mut heights = vec![0.0f32; width * height];
        for y in 0..height {
            for x in 0..width {
                let d = uv_to_dir((x as f32 + 0.5) / width as f32, (y as f32 + 0.5) / height as f32);
                heights[y * width + x] = terrain_height(d * frequency, seed);
            }
        }

        // Altura del vecino: en u da la vuelta (es un círculo), en v se queda en el borde (polos).
        let h_at = |x: isize, y: isize| {
            let x = x.rem_euclid(width as isize) as usize;
            let y = y.clamp(0, height as isize - 1) as usize;
            heights[y * width + x]
        };

        // Cuánto mide un texel sobre una esfera de radio 1 (en el ecuador).
        let du = 2.0 * PI / width as f32;
        let dv = PI / height as f32;

        // --- Paso 2: pendientes con diferencias centrales → normal en espacio tangente ---
        let mut data = Vec::with_capacity(width * height);
        for y in 0..height {
            // Cerca de los polos los texels en u están más juntos: el círculo mide 2π·cos(latitud).
            let lat = (0.5 - (y as f32 + 0.5) / height as f32) * PI;
            let cos_lat = lat.cos().max(0.05);
            for x in 0..width {
                let (xi, yi) = (x as isize, y as isize);
                let slope_u = (h_at(xi + 1, yi) - h_at(xi - 1, yi)) * height_scale / (2.0 * du * cos_lat);
                let slope_v = (h_at(xi, yi + 1) - h_at(xi, yi - 1)) * height_scale / (2.0 * dv);
                // Si el terreno sube hacia +u, la normal se inclina hacia -u (y lo mismo en v).
                data.push(Vec3::new(-slope_u, -slope_v, 1.0).normalized());
            }
        }
        NormalMap { width, height, data }
    }

    /// Lee el mapa en (u, v) con interpolación bilineal (mezcla los 4 texels más cercanos).
    pub fn sample(&self, u: f32, v: f32) -> Vec3 {
        let x = u * self.width as f32 - 0.5;
        let y = v * self.height as f32 - 0.5;
        let (x0, y0) = (x.floor(), y.floor());
        let (fx, fy) = (x - x0, y - y0);
        let texel = |x: i32, y: i32| {
            let x = x.rem_euclid(self.width as i32) as usize;
            let y = y.clamp(0, self.height as i32 - 1) as usize;
            self.data[y * self.width + x]
        };
        let (x0, y0) = (x0 as i32, y0 as i32);
        let top = texel(x0, y0).lerp(texel(x0 + 1, y0), fx);
        let bottom = texel(x0, y0 + 1).lerp(texel(x0 + 1, y0 + 1), fx);
        top.lerp(bottom, fy)
    }

    /// Devuelve la normal del píxel ya perturbada por el mapa (en espacio del mundo).
    /// `strength`: 0 = sin efecto, 1 = el mapa tal cual, 2 = relieve exagerado.
    pub fn apply(&self, f: &Fragment, strength: f32) -> Vec3 {
        let n = f.normal; // normal interpolada de la malla (ya normalizada)
        // T: le quitamos la parte que va en dirección de N (Gram–Schmidt) para que sea perpendicular.
        let t = (f.tangent - n * n.dot(f.tangent)).normalized();
        if t == Vec3::ZERO {
            return n; // en los polos no hay dirección "este": dejamos la normal igual
        }
        let b = n.cross(t); // bitangente: apunta hacia +v (hacia el sur)

        // uv por píxel a partir de la posición local (gira con el planeta y no tiene costura).
        let (u, v) = sphere_uv(f.local_pos.normalized());
        let m = self.sample(u, v);
        let m = Vec3::new(m.x * strength, m.y * strength, m.z).normalized();

        // Matriz TBN: pasar de espacio tangente al mundo.
        (t * m.x + b * m.y + n * m.z).normalized()
    }
}

/// Inversa de `sphere_uv`: de (u, v) a un punto de la esfera de radio 1.
fn uv_to_dir(u: f32, v: f32) -> Vec3 {
    let lon = (u - 0.5) * 2.0 * PI;
    let lat = (0.5 - v) * PI;
    Vec3::new(lat.cos() * lon.cos(), lat.sin(), lat.cos() * lon.sin())
}

/// Altura del relieve fino: rocas (fBm) menos grietas (ruido "ridged": |ruido| hace valles filosos).
fn terrain_height(p: Vec3, seed: u32) -> f32 {
    let rocks = fbm(p, 4, seed);
    let cracks = perlin3(p * 0.5, seed + 1).abs();
    rocks - 0.5 * (1.0 - cracks).powf(8.0)
}
