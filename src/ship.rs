//! Nave + cámara que la sigue (tercera persona).
//! Todo se multiplica por `dt` (segundos del cuadro) para que se mueva igual
//! en computadoras rápidas y lentas.

use crate::math::{Mat4, Vec3};
use crate::scene::Scene;

pub struct Ship {
    pub position: Vec3,
    pub velocity: Vec3,
    /// Giro izquierda/derecha (radianes). yaw = 0 mira hacia -Z.
    pub yaw: f32,
    /// Cabeceo arriba/abajo (radianes).
    pub pitch: f32,
    /// Alabeo visual al girar (solo estético).
    pub roll: f32,
}

/// Lo que el jugador presiona este cuadro (main.rs lo llena desde raylib).
#[derive(Default, Clone, Copy)]
pub struct ShipInput {
    pub thrust: f32, // -1..1  (W / S)
    pub turn: f32,   // -1..1  (A / D)
    pub climb: f32,  // -1..1  (flechas arriba/abajo)
    pub boost: bool, // Shift
    pub brake: bool, // Espacio
}

impl Ship {
    pub fn new(position: Vec3) -> Ship {
        Ship { position, velocity: Vec3::ZERO, yaw: 0.0, pitch: 0.0, roll: 0.0 }
    }

    pub fn forward(&self) -> Vec3 {
        Vec3::new(-self.yaw.sin() * self.pitch.cos(), self.pitch.sin(), -self.yaw.cos() * self.pitch.cos())
    }

    /// Base ortonormal (derecha, arriba, adelante) de la nave.
    pub fn basis(&self) -> (Vec3, Vec3, Vec3) {
        let f = self.forward();
        let r = f.cross(Vec3::UP).normalized();
        let u = r.cross(f);
        (r, u, f)
    }

    pub fn model_matrix(&self, scale: f32) -> Mat4 {
        let (r, u, f) = self.basis();
        Mat4::from_basis(r, u, f, self.position) * Mat4::rotation_z(self.roll) * Mat4::scale(Vec3::splat(scale))
    }

    pub fn update(&mut self, input: ShipInput, dt: f32, scene: &Scene, t: f32) {
        // Giro (TODO navegación: suavizar con velocidad angular para el punto de "giro progresivo").
        let turn_rate = 1.6;
        self.yaw -= input.turn * turn_rate * dt;
        self.pitch = (self.pitch + input.climb * turn_rate * 0.7 * dt).clamp(-1.4, 1.4);
        // Inclinarse un poco al girar.
        let target_roll = -input.turn * 0.5;
        self.roll += (target_roll - self.roll) * (1.0 - (-6.0 * dt).exp());

        // Aceleración progresiva hacia adelante + fricción exponencial.
        let accel = if input.boost { 60.0 } else { 20.0 };
        self.velocity += self.forward() * (input.thrust * accel * dt);
        let drag = if input.brake { 3.0 } else { 0.6 };
        self.velocity = self.velocity * (-drag * dt).exp();
        self.position += self.velocity * dt;

        // Colisión básica: no atravesar planetas ni el sol (empujar hacia afuera).
        for b in &scene.bodies {
            let center = b.position(t);
            let to_ship = self.position - center;
            let min_dist = b.radius * 1.12 + 0.8;
            let d = to_ship.length();
            if d < min_dist {
                let n = to_ship / d.max(1e-4);
                self.position = center + n * min_dist;
                let into = self.velocity.dot(n);
                if into < 0.0 {
                    self.velocity -= n * into; // quitar la velocidad que va hacia adentro
                }
            }
        }
    }
}

/// Cámara de persecución: se coloca detrás y arriba de la nave y la sigue con suavizado.
pub struct ChaseCamera {
    pub position: Vec3,
    pub target: Vec3,
    pub distance: f32,
    pub height: f32,
}

impl ChaseCamera {
    pub fn new(ship: &Ship) -> ChaseCamera {
        let mut c = ChaseCamera { position: ship.position, target: ship.position, distance: 7.0, height: 2.2 };
        c.update(ship, 1.0);
        c
    }

    pub fn update(&mut self, ship: &Ship, dt: f32) {
        let (_, up, f) = ship.basis();
        let desired = ship.position - f * self.distance + up * self.height;
        let k = 1.0 - (-8.0 * dt).exp(); // suavizado independiente de los FPS
        self.position = self.position.lerp(desired, k);
        self.target = ship.position + f * 6.0;
    }

    pub fn view(&self) -> Mat4 {
        Mat4::look_at(self.position, self.target, Vec3::UP)
    }

    /// (derecha, arriba, adelante) de la cámara.
    pub fn basis(&self) -> (Vec3, Vec3, Vec3) {
        let f = (self.target - self.position).normalized();
        let r = f.cross(Vec3::UP).normalized();
        (r, r.cross(f), f)
    }
}
