//! Matemática básica: vectores y matrices 4x4 escritas por nosotros.
//!
//! Convenciones (iguales a OpenGL):
//! - Sistema de mano derecha: +X derecha, +Y arriba, la cámara mira hacia -Z.
//! - Las matrices se guardan por filas: `m[fila][columna]`.
//! - Los vectores son columnas: `v' = M * v`. Por eso el orden es P * V * M * v.

use std::ops::{Add, AddAssign, Div, Mul, Neg, Sub, SubAssign};

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Vec3 {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

impl Vec3 {
    pub const ZERO: Vec3 = Vec3 { x: 0.0, y: 0.0, z: 0.0 };
    pub const UP: Vec3 = Vec3 { x: 0.0, y: 1.0, z: 0.0 };

    pub const fn new(x: f32, y: f32, z: f32) -> Self {
        Vec3 { x, y, z }
    }
    pub fn splat(v: f32) -> Self {
        Vec3::new(v, v, v)
    }
    pub fn dot(self, o: Vec3) -> f32 {
        self.x * o.x + self.y * o.y + self.z * o.z
    }
    pub fn cross(self, o: Vec3) -> Vec3 {
        Vec3::new(
            self.y * o.z - self.z * o.y,
            self.z * o.x - self.x * o.z,
            self.x * o.y - self.y * o.x,
        )
    }
    pub fn length(self) -> f32 {
        self.dot(self).sqrt()
    }
    pub fn normalized(self) -> Vec3 {
        let l = self.length();
        if l > 1e-8 { self / l } else { Vec3::ZERO }
    }
    pub fn lerp(self, o: Vec3, t: f32) -> Vec3 {
        self + (o - self) * t
    }
    /// Multiplicación componente a componente (útil para colores).
    pub fn mul_elem(self, o: Vec3) -> Vec3 {
        Vec3::new(self.x * o.x, self.y * o.y, self.z * o.z)
    }
    pub fn clamp01(self) -> Vec3 {
        Vec3::new(self.x.clamp(0.0, 1.0), self.y.clamp(0.0, 1.0), self.z.clamp(0.0, 1.0))
    }
}

impl Add for Vec3 { type Output = Vec3; fn add(self, o: Vec3) -> Vec3 { Vec3::new(self.x + o.x, self.y + o.y, self.z + o.z) } }
impl Sub for Vec3 { type Output = Vec3; fn sub(self, o: Vec3) -> Vec3 { Vec3::new(self.x - o.x, self.y - o.y, self.z - o.z) } }
impl Mul<f32> for Vec3 { type Output = Vec3; fn mul(self, s: f32) -> Vec3 { Vec3::new(self.x * s, self.y * s, self.z * s) } }
impl Div<f32> for Vec3 { type Output = Vec3; fn div(self, s: f32) -> Vec3 { Vec3::new(self.x / s, self.y / s, self.z / s) } }
impl Neg for Vec3 { type Output = Vec3; fn neg(self) -> Vec3 { Vec3::new(-self.x, -self.y, -self.z) } }
impl AddAssign for Vec3 { fn add_assign(&mut self, o: Vec3) { *self = *self + o; } }
impl SubAssign for Vec3 { fn sub_assign(&mut self, o: Vec3) { *self = *self - o; } }

/// Vector homogéneo (x, y, z, w). Sale de multiplicar por la matriz de proyección.
#[derive(Clone, Copy, Debug, Default)]
pub struct Vec4 {
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub w: f32,
}

impl Vec4 {
    pub fn new(x: f32, y: f32, z: f32, w: f32) -> Self {
        Vec4 { x, y, z, w }
    }
    pub fn lerp(self, o: Vec4, t: f32) -> Vec4 {
        Vec4::new(
            self.x + (o.x - self.x) * t,
            self.y + (o.y - self.y) * t,
            self.z + (o.z - self.z) * t,
            self.w + (o.w - self.w) * t,
        )
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Mat4 {
    pub m: [[f32; 4]; 4],
}

impl Mat4 {
    pub fn identity() -> Mat4 {
        let mut m = [[0.0; 4]; 4];
        for (i, row) in m.iter_mut().enumerate() {
            row[i] = 1.0;
        }
        Mat4 { m }
    }

    pub fn translation(t: Vec3) -> Mat4 {
        let mut r = Mat4::identity();
        r.m[0][3] = t.x;
        r.m[1][3] = t.y;
        r.m[2][3] = t.z;
        r
    }

    pub fn scale(s: Vec3) -> Mat4 {
        let mut r = Mat4::identity();
        r.m[0][0] = s.x;
        r.m[1][1] = s.y;
        r.m[2][2] = s.z;
        r
    }

    pub fn rotation_x(a: f32) -> Mat4 {
        let (s, c) = a.sin_cos();
        let mut r = Mat4::identity();
        r.m[1][1] = c; r.m[1][2] = -s;
        r.m[2][1] = s; r.m[2][2] = c;
        r
    }

    pub fn rotation_y(a: f32) -> Mat4 {
        let (s, c) = a.sin_cos();
        let mut r = Mat4::identity();
        r.m[0][0] = c;  r.m[0][2] = s;
        r.m[2][0] = -s; r.m[2][2] = c;
        r
    }

    pub fn rotation_z(a: f32) -> Mat4 {
        let (s, c) = a.sin_cos();
        let mut r = Mat4::identity();
        r.m[0][0] = c; r.m[0][1] = -s;
        r.m[1][0] = s; r.m[1][1] = c;
        r
    }

    /// Matriz de modelo a partir de una base ortonormal (derecha, arriba, adelante) y una posición.
    /// "Adelante" es hacia donde apunta la nariz del objeto; en espacio local es -Z.
    pub fn from_basis(right: Vec3, up: Vec3, forward: Vec3, pos: Vec3) -> Mat4 {
        let back = -forward;
        Mat4 {
            m: [
                [right.x, up.x, back.x, pos.x],
                [right.y, up.y, back.y, pos.y],
                [right.z, up.z, back.z, pos.z],
                [0.0, 0.0, 0.0, 1.0],
            ],
        }
    }

    /// Matriz de proyección en perspectiva (estilo OpenGL, z de clip en [-w, w]).
    pub fn perspective(fov_y: f32, aspect: f32, near: f32, far: f32) -> Mat4 {
        let f = 1.0 / (fov_y * 0.5).tan();
        let mut r = Mat4 { m: [[0.0; 4]; 4] };
        r.m[0][0] = f / aspect;
        r.m[1][1] = f;
        r.m[2][2] = (far + near) / (near - far);
        r.m[2][3] = (2.0 * far * near) / (near - far);
        r.m[3][2] = -1.0;
        r
    }

    /// Matriz de vista: mueve el mundo para que la cámara quede en el origen mirando a -Z.
    pub fn look_at(eye: Vec3, target: Vec3, up: Vec3) -> Mat4 {
        let f = (target - eye).normalized(); // adelante
        let r = f.cross(up).normalized(); // derecha
        let u = r.cross(f); // arriba real
        Mat4 {
            m: [
                [r.x, r.y, r.z, -r.dot(eye)],
                [u.x, u.y, u.z, -u.dot(eye)],
                [-f.x, -f.y, -f.z, f.dot(eye)],
                [0.0, 0.0, 0.0, 1.0],
            ],
        }
    }

    pub fn mul(&self, o: &Mat4) -> Mat4 {
        let mut r = [[0.0; 4]; 4];
        for (i, row) in r.iter_mut().enumerate() {
            for (j, cell) in row.iter_mut().enumerate() {
                *cell = (0..4).map(|k| self.m[i][k] * o.m[k][j]).sum();
            }
        }
        Mat4 { m: r }
    }

    pub fn transform_vec4(&self, v: Vec4) -> Vec4 {
        let m = &self.m;
        Vec4::new(
            m[0][0] * v.x + m[0][1] * v.y + m[0][2] * v.z + m[0][3] * v.w,
            m[1][0] * v.x + m[1][1] * v.y + m[1][2] * v.z + m[1][3] * v.w,
            m[2][0] * v.x + m[2][1] * v.y + m[2][2] * v.z + m[2][3] * v.w,
            m[3][0] * v.x + m[3][1] * v.y + m[3][2] * v.z + m[3][3] * v.w,
        )
    }

    /// Transforma un punto (w = 1, sí le afecta la traslación).
    pub fn transform_point(&self, p: Vec3) -> Vec3 {
        let v = self.transform_vec4(Vec4::new(p.x, p.y, p.z, 1.0));
        Vec3::new(v.x, v.y, v.z)
    }

    /// Transforma una dirección (w = 0, ignora la traslación).
    /// Para normales esto es correcto mientras la escala sea uniforme.
    pub fn transform_dir(&self, d: Vec3) -> Vec3 {
        let v = self.transform_vec4(Vec4::new(d.x, d.y, d.z, 0.0));
        Vec3::new(v.x, v.y, v.z)
    }
}

impl Mul for Mat4 {
    type Output = Mat4;
    fn mul(self, o: Mat4) -> Mat4 {
        Mat4::mul(&self, &o)
    }
}
