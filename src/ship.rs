//! Nave + cámara que la sigue (tercera persona).
//! Todo se multiplica por `dt` (segundos del cuadro) para que se mueva igual
//! en computadoras rápidas y lentas.

use crate::math::{Mat4, Vec3};
use crate::noise::hash01;
use crate::renderer::{Fragment, FragmentShader, Uniforms};
use crate::scene::Scene;

// --- look del halcon, estos son los numeros q se pueden mover en vivo ---
// color base del casco, un gris metalico clarito
const HULL_COLOR: Vec3 = Vec3 { x: 0.62, y: 0.62, z: 0.60 };
// color de las rayitas entre paneles, mas oscuro q el casco
const PANEL_LINE_COLOR: Vec3 = Vec3 { x: 0.16, y: 0.16, z: 0.17 };
// tamaño de cada panel en coordenadas de la nave (la nave mide como 2.3 de largo)
const PANEL_SIZE: f32 = 0.24;
// grosor de la rayita, como fraccion del panel
const PANEL_LINE_WIDTH: f32 = 0.10;
// cuanto cambia el tono de un panel a otro pa q no se vea todo igual
const PANEL_TONE_VARIATION: f32 = 0.22;
// q tan cerrado es el brillo especular, mas alto = puntito mas chico
const HULL_SHININESS: f32 = 40.0;
// color del brillo de los motores
const ENGINE_COLOR: Vec3 = Vec3 { x: 0.35, y: 0.65, z: 1.0 };
// desde donde empiezan los motores en z local (la parte de atras de la nave es +z)
const ENGINE_Z_START: f32 = 0.60;
const ENGINE_Z_FULL: f32 = 0.78;
// los motores son una franja angosta en y, aca va el centro y la mitad del alto
const ENGINE_Y_CENTER: f32 = -0.033;
const ENGINE_Y_HALF: f32 = 0.07;
// el brillo q tienen los motores con la nave quieta y lo maximo q pueden brillar
const ENGINE_IDLE_GLOW: f32 = 0.25;
const ENGINE_MAX_GLOW: f32 = 1.6;
// a esta velocidad los motores ya estan a todo
const ENGINE_FULL_SPEED: f32 = 40.0;

pub struct Ship {
    pub position: Vec3,
    pub velocity: Vec3,
    /// Giro izquierda/derecha (radianes). yaw = 0 mira hacia -Z.
    pub yaw: f32,
    /// Cabeceo arriba/abajo (radianes).
    pub pitch: f32,
    /// Alabeo visual al girar (solo estético).
    pub roll: f32,
    /// Turbo (0..1), suavizado. Controla la llama de atrás.
    pub boost: f32,
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
        Ship { position, velocity: Vec3::ZERO, yaw: 0.0, pitch: 0.0, roll: 0.0, boost: 0.0 }
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

    /// cuanto deben brillar los motores, de 0 (quieta) a 1 (a toda velocidad o con turbo)
    pub fn engine_glow(&self) -> f32 {
        // el turbo prende los motores de una aunque la nave todavia no agarre velocidad
        (self.velocity.length() / ENGINE_FULL_SPEED).max(self.boost).min(1.0)
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
        // El turbo (Shift) empuja hacia adelante aunque no se presione W.
        let thrust = if input.boost { 1.0 } else { input.thrust };
        let accel = if input.boost { 60.0 } else { 20.0 };
        self.velocity += self.forward() * (thrust * accel * dt);

        // Turbo: sube y baja suave (independiente de los FPS). Enciende rápido y se apaga
        // un poco más lento, como si la llama se fuera consumiendo.
        let boost_target = if input.boost { 1.0 } else { 0.0 };
        let boost_rate = if boost_target > self.boost { 8.0 } else { 3.0 };
        self.boost += (boost_target - self.boost) * (1.0 - (-boost_rate * dt).exp());
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

// pasa de 0 a 1 suavecito entre a y b, lo uso pa q los bordes no se vean cortados
fn smoothstep(a: f32, b: f32, x: f32) -> f32 {
    let t = ((x - a) / (b - a)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// shader de la nave: casco gris con paneles + motores azules atras q crecen con la velocidad
pub struct HalconShader {
    /// viene de Ship::engine_glow, de 0 a 1
    pub engine_glow: f32,
}

impl FragmentShader for HalconShader {
    fn shade(&self, f: &Fragment, u: &Uniforms) -> Vec3 {
        // paso 1: partir la nave en cuadritos usando x y z locales
        // uso local_pos y no world_pos pa q los paneles se muevan pegados a la nave
        let cell_x = f.local_pos.x / PANEL_SIZE;
        let cell_z = f.local_pos.z / PANEL_SIZE;

        // paso 2: a cada cuadrito le toca un tono distinto segun su hash
        // el hash siempre da lo mismo pa la misma celda, entonces el tono no parpadea
        let tone = 1.0 + (hash01(cell_x.floor() as i32, 0, cell_z.floor() as i32, 31) - 0.5) * PANEL_TONE_VARIATION;
        let plate = HULL_COLOR * tone;

        // paso 3: si estamos muy cerca del borde del cuadrito pintamos la rayita oscura
        // (cell - floor) da de 0 a 1 dentro del cuadrito, y lo comparo contra los dos lados
        let (sx, sz) = (cell_x - cell_x.floor(), cell_z - cell_z.floor());
        let edge = sx.min(1.0 - sx).min(sz).min(1.0 - sz);
        let line = 1.0 - smoothstep(0.0, PANEL_LINE_WIDTH, edge);
        let base = plate.lerp(PANEL_LINE_COLOR, line);

        // paso 4: luz del sol con blinn-phong
        // l apunta al sol, v a la camara, y h es el punto medio entre los dos
        let l = (u.light_pos - f.world_pos).normalized();
        let v = (u.camera_pos - f.world_pos).normalized();
        let h = (l + v).normalized();
        let diff = f.normal.dot(l).max(0.0);
        let spec = f.normal.dot(h).max(0.0).powf(HULL_SHININESS);

        // paso 5: un poquito de luz en el borde pa q no desaparezca del lado oscuro
        let rim = (1.0 - f.normal.dot(v).max(0.0)).powf(3.0) * 0.25;
        let lit = base * (0.15 + 0.85 * diff) + Vec3::splat(spec * 0.6) + Vec3::new(0.5, 0.6, 0.8) * rim;

        // paso 6: los motores, solo brilla lo q este atras y en la franja de la altura de los motores
        let back = smoothstep(ENGINE_Z_START, ENGINE_Z_FULL, f.local_pos.z);
        let band = 1.0 - smoothstep(ENGINE_Y_HALF, ENGINE_Y_HALF * 1.6, (f.local_pos.y - ENGINE_Y_CENTER).abs());

        // paso 7: entre mas rapido va la nave mas fuerte el brillo, y se suma encima de la luz
        // no depende del sol, por eso tambien se ve del lado oscuro
        let glow = ENGINE_IDLE_GLOW + (ENGINE_MAX_GLOW - ENGINE_IDLE_GLOW) * self.engine_glow;
        lit + ENGINE_COLOR * (back * band * glow)
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
