//! Ruido procedural propio (ruido de gradiente tipo Perlin en 3D + fBm).
//!
//! Todo depende de una SEMILLA: la misma semilla siempre da el mismo resultado.
//! Se usa para: color de los planetas, deformar su superficie, estrellas del fondo,
//! y (más adelante) distribuir árboles/rocas sobre los planetas.

use crate::math::Vec3;

/// Hash entero rápido y determinista (mezcla de bits). Devuelve un u32 "aleatorio".
#[inline]
pub fn hash3(x: i32, y: i32, z: i32, seed: u32) -> u32 {
    let mut h = seed
        ^ (x as u32).wrapping_mul(0x8da6_b343)
        ^ (y as u32).wrapping_mul(0xd816_3841)
        ^ (z as u32).wrapping_mul(0xcb1a_b31f);
    h ^= h >> 13;
    h = h.wrapping_mul(0x5bd1_e995);
    h ^= h >> 15;
    h
}

/// Número pseudoaleatorio en [0, 1) a partir de una celda entera.
#[inline]
pub fn hash01(x: i32, y: i32, z: i32, seed: u32) -> f32 {
    (hash3(x, y, z, seed) & 0x00ff_ffff) as f32 / 16_777_216.0
}

/// Uno de 12 gradientes (aristas de un cubo), elegido por el hash.
#[inline]
fn grad(h: u32, x: f32, y: f32, z: f32) -> f32 {
    match h % 12 {
        0 => x + y, 1 => -x + y, 2 => x - y, 3 => -x - y,
        4 => x + z, 5 => -x + z, 6 => x - z, 7 => -x - z,
        8 => y + z, 9 => -y + z, 10 => y - z, _ => -y - z,
    }
}

/// Curva de suavizado de Perlin: 6t^5 - 15t^4 + 10t^3
#[inline]
fn fade(t: f32) -> f32 {
    t * t * t * (t * (t * 6.0 - 15.0) + 10.0)
}

#[inline]
fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

/// Ruido de gradiente 3D. Devuelve aprox. [-1, 1].
pub fn perlin3(p: Vec3, seed: u32) -> f32 {
    let (xi, yi, zi) = (p.x.floor() as i32, p.y.floor() as i32, p.z.floor() as i32);
    let (xf, yf, zf) = (p.x - xi as f32, p.y - yi as f32, p.z - zi as f32);
    let (u, v, w) = (fade(xf), fade(yf), fade(zf));

    let g = |dx: i32, dy: i32, dz: i32| {
        grad(
            hash3(xi + dx, yi + dy, zi + dz, seed),
            xf - dx as f32,
            yf - dy as f32,
            zf - dz as f32,
        )
    };

    let x00 = lerp(g(0, 0, 0), g(1, 0, 0), u);
    let x10 = lerp(g(0, 1, 0), g(1, 1, 0), u);
    let x01 = lerp(g(0, 0, 1), g(1, 0, 1), u);
    let x11 = lerp(g(0, 1, 1), g(1, 1, 1), u);
    lerp(lerp(x00, x10, v), lerp(x01, x11, v), w)
}

/// fBm (fractal Brownian motion): suma de varias "octavas" de ruido,
/// cada una con el doble de frecuencia y la mitad de amplitud.
pub fn fbm(p: Vec3, octaves: u32, seed: u32) -> f32 {
    let mut sum = 0.0;
    let mut amp = 0.5;
    let mut freq = 1.0;
    for i in 0..octaves {
        sum += amp * perlin3(p * freq, seed.wrapping_add(i * 1013));
        freq *= 2.0;
        amp *= 0.5;
    }
    sum
}
