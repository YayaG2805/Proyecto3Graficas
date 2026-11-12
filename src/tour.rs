//! TOUR AUTOMÁTICO (tecla T): un piloto que hace solo el recorrido que pide la entrega,
//! para grabar el video o mostrar la demo sin tener que pilotear.
//!
//! No mueve la nave por su cuenta: en cada cuadro decide qué TECLAS "apretar" (un `ShipInput`)
//! y el juego las procesa igual que si las apretara una persona. Por eso usa el mismo vuelo,
//! warp, órbita y colisiones que el jugador. Las fases se repiten mundo tras mundo:
//!
//!   1. VUELO ESPACIAL:  vuela unos segundos de lado respecto al Sol, subiendo un poco.
//!   2. VIAJE:           elige el siguiente mundo (1-5) y aprieta Tab (warp).
//!   3. ACERCAMIENTO:    al llegar aprieta F para engancharse a la órbita; si quedó lejos,
//!      primero se acerca volando.
//!   4. VUELTA:          escanea (E), baja pegado a la superficie y da una vuelta completa.
//!   5. ALEJAMIENTO:     suelta la órbita (F) y se aleja con turbo.
//!
//! Cualquier tecla de vuelo del jugador lo detiene (eso lo revisa main.rs).

use crate::interaction::{orbit_in_range, GameState};
use crate::math::Vec3;
use crate::scene::Scene;
use crate::ship::{Ship, ShipInput};

/// Segundos de vuelo libre antes de cada viaje.
const CRUISE_TIME: f32 = 3.0;
/// En el vuelo libre acelera solo si la nariz ya apunta a donde va (coseno del ángulo: 0.7 ≈ 45°).
const CRUISE_FACING: f32 = 0.7;
/// Qué tanto sube en el vuelo libre (0 = plano).
const CRUISE_RISE: f32 = 0.25;
/// Si no logra engancharse en este tiempo, sigue con el siguiente mundo.
const APPROACH_MAX_TIME: f32 = 12.0;
/// Velocidad máxima al acercarse volando (sin warp).
const APPROACH_SPEED: f32 = 15.0;
/// En la vuelta baja hasta esta altura (en radios del mundo) para ver bien la superficie.
const ORBIT_LOW_RADII: f32 = 0.35;
/// Por si algo falla, la vuelta se corta después de esto.
const ORBIT_MAX_TIME: f32 = 30.0;
/// Segundos alejándose con turbo, y cuántos de ellos subiendo la nariz.
const DEPART_TIME: f32 = 4.0;
const DEPART_CLIMB_TIME: f32 = 1.2;
/// Qué tan fuerte corrige el rumbo al acercarse (más alto = gira más brusco).
const STEER_GAIN: f32 = 3.0;

/// Los mundos que visita, en orden (las teclas 1-5).
const WORLDS: [usize; 5] = [1, 2, 3, 4, 5];

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Phase {
    Cruise,
    Travel,
    Approach,
    Orbit,
    Depart,
}

pub struct Tour {
    pub phase: Phase,
    /// Segundos en la fase actual.
    timer: f32,
    /// Lugar en `WORLDS` del mundo que va a visitar.
    next: usize,
    /// Cuánto ha girado alrededor del mundo en esta vuelta (radianes) y desde dónde.
    lap: f32,
    last_dir: Option<Vec3>,
    /// Mundos que ya rodeó completos (para los tests y el HUD).
    pub laps_done: u32,
}

/// Decide giro y cabeceo para apuntar la nariz hacia `dir` (de largo 1).
fn steer(ship: &Ship, dir: Vec3) -> (f32, f32) {
    let (right, up, _) = ship.basis();
    ((dir.dot(right) * STEER_GAIN).clamp(-1.0, 1.0), (dir.dot(up) * STEER_GAIN).clamp(-1.0, 1.0))
}

impl Tour {
    /// Empieza el tour; el primer mundo es el destino que ya estaba elegido.
    pub fn new(first_world: usize) -> Tour {
        let next = WORLDS.iter().position(|&w| w == first_world).unwrap_or(0);
        Tour { phase: Phase::Cruise, timer: 0.0, next, lap: 0.0, last_dir: None, laps_done: 0 }
    }

    pub fn world(&self) -> usize {
        WORLDS[self.next]
    }

    fn go(&mut self, phase: Phase) {
        self.phase = phase;
        self.timer = 0.0;
    }

    /// Para el HUD: qué está haciendo el tour.
    pub fn describe(&self, scene: &Scene) -> String {
        let name = scene.bodies.get(self.world()).map_or("?", |b| b.name);
        match self.phase {
            Phase::Cruise => "vuelo espacial".to_string(),
            Phase::Travel => format!("viaje a {name}"),
            Phase::Approach => format!("acercamiento a {name}"),
            Phase::Orbit => format!("vuelta alrededor de {name}"),
            Phase::Depart => format!("alejandose de {name}"),
        }
    }

    /// Las teclas que "aprieta" el piloto en este cuadro.
    pub fn drive(&mut self, ship: &Ship, state: &GameState, scene: &Scene, t: f32, dt: f32) -> ShipInput {
        let first_frame = self.timer == 0.0;
        self.timer += dt;
        let world = self.world();
        let center = scene.body_position(world, t);
        let radius = scene.bodies[world].radius;
        let mut input = ShipInput::default();

        match self.phase {
            Phase::Cruise => {
                // vuela de lado respecto al Sol (como siguiendo una órbita) y subiendo un poco;
                // el viaje al mundo lo hace el warp. Acelera solo cuando ya va en esa dirección
                // (al empezar el juego la nave mira al Sol: acelerar recto sería quemarse).
                let from_sun = ship.position.normalized(); // el Sol está en el origen
                let mut side = Vec3::UP.cross(from_sun).normalized();
                if side.dot(ship.forward()) < 0.0 {
                    side = -side; // el lado que no obliga a dar media vuelta
                }
                let dir = (side + Vec3::UP * CRUISE_RISE).normalized();
                let (turn, climb) = steer(ship, dir);
                input.turn = turn;
                input.climb = climb;
                input.thrust = if ship.forward().dot(dir) > CRUISE_FACING { 1.0 } else { 0.0 };
                if self.timer >= CRUISE_TIME {
                    self.go(Phase::Travel);
                }
            }
            Phase::Travel => {
                if first_frame {
                    // el destino se elige antes que el warp en el mismo cuadro (ver interaction.rs)
                    input.select = Some(world);
                    input.warp = true;
                } else if !ship.warping() {
                    // llegó (o ya estaba tan cerca que el warp no arrancó)
                    self.go(Phase::Approach);
                }
            }
            Phase::Approach => {
                if state.orbit_lock == Some(world) {
                    self.lap = 0.0;
                    self.last_dir = None;
                    self.go(Phase::Orbit);
                } else if state.nearest == Some(world) && orbit_in_range(state, scene) {
                    input.orbit_toggle = true;
                } else {
                    // quedó lejos: vuela hacia el mundo, sin pasarse de velocidad
                    let (turn, climb) = steer(ship, (center - ship.position).normalized());
                    input.turn = turn;
                    input.climb = climb;
                    input.thrust = if ship.velocity.length() < APPROACH_SPEED { 1.0 } else { 0.0 };
                }
                if self.timer > APPROACH_MAX_TIME {
                    self.go(Phase::Depart);
                }
            }
            Phase::Orbit => {
                if first_frame {
                    input.scan = true; // muestra la ficha del mundo
                }
                input.thrust = 1.0;
                if state.nearest == Some(world) && state.nearest_dist > ORBIT_LOW_RADII * radius {
                    input.climb = -1.0;
                }
                // cuánto giró: el ángulo entre la dirección del centro a la nave ahora y antes
                let dir = (ship.position - center).normalized();
                if let Some(last) = self.last_dir {
                    self.lap += last.dot(dir).clamp(-1.0, 1.0).acos();
                }
                self.last_dir = Some(dir);
                let done = self.lap >= std::f32::consts::TAU;
                if done {
                    self.laps_done += 1;
                }
                if done || self.timer > ORBIT_MAX_TIME || state.orbit_lock.is_none() {
                    self.go(Phase::Depart);
                }
            }
            Phase::Depart => {
                if first_frame && state.orbit_lock.is_some() {
                    input.orbit_toggle = true; // suelta la órbita
                } else {
                    input.boost = true;
                    if self.timer < DEPART_CLIMB_TIME {
                        input.climb = 1.0;
                    }
                }
                if self.timer >= DEPART_TIME {
                    self.next = (self.next + 1) % WORLDS.len();
                    self.go(Phase::Cruise);
                }
            }
        }
        input
    }
}

/// ¿El jugador apretó alguna tecla de vuelo? Entonces el tour se detiene y él toma el control.
/// (P, el modo foto, no cuenta: se puede pausar el tour para tomar una foto.)
pub fn player_took_control(input: &ShipInput) -> bool {
    input.thrust != 0.0
        || input.turn != 0.0
        || input.climb != 0.0
        || input.boost
        || input.brake
        || input.orbit_toggle
        || input.scan
        || input.warp
        || input.select.is_some()
        || input.gravity_toggle
}
