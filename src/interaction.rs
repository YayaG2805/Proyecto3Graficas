//! todo lo q pasa entre la nave y los mundos: cual esta mas cerca, escanear, elegir destino...
//!
//! main.rs llama a `update` una vez por cuadro y el hud solo lee lo q queda en `GameState`.

use crate::scene::Scene;
use crate::ship::{Ship, ShipInput};

// se puede escanear un mundo si estas a menos de esta cantidad de radios de su superficie
const SCAN_RANGE_RADII: f32 = 3.0;
// el primer mundo q se elige como destino (bodies[0] es el sol)
const FIRST_WORLD: usize = 1;

pub struct GameState {
    /// cuerpo mas cercano a la nave
    pub nearest: Option<usize>,
    /// distancia de la nave a la superficie de ese cuerpo
    pub nearest_dist: f32,
    /// la ficha q se muestra en el hud
    pub scanned: Option<usize>,
    /// a q mundo se va a saltar con el warp
    pub warp_target: usize,
    /// intensidad del efecto warp, de 0 a 1
    pub warp: f32,
    /// el cuerpo al q estamos enganchados en modo orbita
    pub orbit_lock: Option<usize>,
    pub collected: u32,
    pub total: u32,
    pub photo_mode: bool,
}

impl GameState {
    pub fn new() -> GameState {
        GameState {
            nearest: None,
            nearest_dist: f32::MAX,
            scanned: None,
            warp_target: FIRST_WORLD,
            warp: 0.0,
            orbit_lock: None,
            collected: 0,
            total: 0,
            photo_mode: false,
        }
    }
}

// busca el cuerpo cuya superficie esta mas cerca de la nave
fn find_nearest(ship: &Ship, scene: &Scene, t: f32) -> (Option<usize>, f32) {
    let mut nearest = None;
    let mut nearest_dist = f32::MAX;
    for (i, b) in scene.bodies.iter().enumerate() {
        // distancia al centro menos el radio = distancia a la superficie
        // sin restar el radio un planeta grande lejos podria perder contra uno chiquito
        let dist = (scene.body_position(i, t) - ship.position).length() - b.radius;
        if dist < nearest_dist {
            nearest = Some(i);
            nearest_dist = dist;
        }
    }
    (nearest, nearest_dist)
}

/// se llama una vez por cuadro, antes de mover la camara
pub fn update(state: &mut GameState, ship: &mut Ship, scene: &Scene, input: &ShipInput, t: f32, _dt: f32) {
    // paso 1: ver cual cuerpo tenemos mas cerca, lo demas depende de esto
    let (nearest, nearest_dist) = find_nearest(ship, scene, t);
    state.nearest = nearest;
    state.nearest_dist = nearest_dist;

    // paso 2: con E guardamos la ficha, pero solo si estamos lo bastante cerca
    if input.scan {
        if let Some(i) = nearest {
            if nearest_dist < SCAN_RANGE_RADII * scene.bodies[i].radius {
                state.scanned = Some(i);
            }
        }
    }

    // paso 3: las teclas 1 a 5 cambian el destino
    // si ese mundo no existe o es una luna la tecla no hace nada, asi las lunas no corren los numeros
    if let Some(i) = input.select {
        if i >= FIRST_WORLD && i < scene.bodies.len() && scene.bodies[i].parent.is_none() {
            state.warp_target = i;
        }
    }

    // paso 4: con P entramos o salimos del modo foto
    if input.photo {
        state.photo_mode = !state.photo_mode;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::math::Vec3;

    // pongo la nave a cierta altura sobre la superficie de un mundo, en el segundo 0
    fn ship_above(scene: &Scene, i: usize, height: f32) -> Ship {
        let center = scene.body_position(i, 0.0);
        Ship::new(center + Vec3::new(0.0, scene.bodies[i].radius + height, 0.0))
    }

    #[test]
    fn encuentra_el_cuerpo_mas_cercano_y_mide_desde_la_superficie() {
        let scene = Scene::new();
        let mut state = GameState::new();
        let mut ship = ship_above(&scene, 2, 5.0);
        update(&mut state, &mut ship, &scene, &ShipInput::default(), 0.0, 0.016);
        assert_eq!(state.nearest, Some(2));
        assert!((state.nearest_dist - 5.0).abs() < 1e-3);
    }

    #[test]
    fn solo_escanea_si_esta_cerca_y_aprieta_la_tecla() {
        let scene = Scene::new();
        let mut state = GameState::new();
        let scan = ShipInput { scan: true, ..Default::default() };
        let radius = scene.bodies[1].radius;

        // cerca pero sin apretar nada
        let mut ship = ship_above(&scene, 1, radius);
        update(&mut state, &mut ship, &scene, &ShipInput::default(), 0.0, 0.016);
        assert_eq!(state.scanned, None);

        // apretando pero lejos del rango, sigue siendo el mas cercano
        let mut ship = ship_above(&scene, 1, radius * 3.5);
        update(&mut state, &mut ship, &scene, &scan, 0.0, 0.016);
        assert_eq!(state.nearest, Some(1));
        assert_eq!(state.scanned, None);

        // apretando y cerca
        let mut ship = ship_above(&scene, 1, radius);
        update(&mut state, &mut ship, &scene, &scan, 0.0, 0.016);
        assert_eq!(state.scanned, Some(1));
    }

    #[test]
    fn el_destino_ignora_lo_q_no_es_un_mundo() {
        let mut scene = Scene::new();
        let mut state = GameState::new();
        let mut ship = ship_above(&scene, 1, 5.0);
        let select = |i| ShipInput { select: Some(i), ..Default::default() };

        update(&mut state, &mut ship, &scene, &select(3), 0.0, 0.016);
        assert_eq!(state.warp_target, 3);

        // el sol no es destino
        update(&mut state, &mut ship, &scene, &select(0), 0.0, 0.016);
        assert_eq!(state.warp_target, 3);

        // un mundo q no existe tampoco
        update(&mut state, &mut ship, &scene, &select(99), 0.0, 0.016);
        assert_eq!(state.warp_target, 3);

        // una luna tampoco
        scene.bodies[4].parent = Some(3);
        update(&mut state, &mut ship, &scene, &select(4), 0.0, 0.016);
        assert_eq!(state.warp_target, 3);
    }

    #[test]
    fn la_p_prende_y_apaga_el_modo_foto() {
        let scene = Scene::new();
        let mut state = GameState::new();
        let mut ship = ship_above(&scene, 1, 5.0);
        let photo = ShipInput { photo: true, ..Default::default() };

        update(&mut state, &mut ship, &scene, &photo, 0.0, 0.016);
        assert!(state.photo_mode);
        update(&mut state, &mut ship, &scene, &photo, 0.0, 0.016);
        assert!(!state.photo_mode);
    }
}
