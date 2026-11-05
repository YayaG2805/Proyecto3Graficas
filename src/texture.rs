//! Textura propia: una imagen de colores (Vec3) que se lee con coordenadas (u, v) en [0, 1].
//! Se usa para guardar cosas que cuesta calcular y no cambian: el mapa de normales y
//! la nebulosa del cielo. Se piensan como textura EQUIRECTANGULAR (u = longitud, v = latitud),
//! por eso en u se da la vuelta (es un círculo) y en v se queda en el borde (polos).

use crate::math::Vec3;

pub struct Texture {
    width: usize,
    height: usize,
    data: Vec<Vec3>,
}

impl Texture {
    /// Crea la textura con datos ya calculados (fila por fila).
    pub fn new(width: usize, height: usize, data: Vec<Vec3>) -> Texture {
        assert_eq!(data.len(), width * height);
        Texture { width, height, data }
    }

    /// Crea la textura evaluando `f(u, v)` en el centro de cada texel.
    pub fn from_fn(width: usize, height: usize, f: impl Fn(f32, f32) -> Vec3) -> Texture {
        let mut data = Vec::with_capacity(width * height);
        for y in 0..height {
            for x in 0..width {
                data.push(f((x as f32 + 0.5) / width as f32, (y as f32 + 0.5) / height as f32));
            }
        }
        Texture { width, height, data }
    }

    /// Lee la textura en (u, v) con interpolación bilineal (mezcla los 4 texels más cercanos).
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
}
