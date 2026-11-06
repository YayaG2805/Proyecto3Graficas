//! todo lo q pasa entre la nave y los mundos: cual esta mas cerca, escanear, elegir destino...
//!
//! main.rs llama a `update` una vez por cuadro y el hud solo lee lo q queda en `GameState`.

use crate::scene::Scene;
use crate::ship::{Ship, ShipInput};

// se puede escanear un mundo si estas a menos de esta cantidad de radios de su superficie
const SCAN_RANGE_RADII: f32 = 3.0;
// se puede enganchar la orbita si estas a menos de esta cantidad de radios de la superficie
const ORBIT_RANGE_RADII: f32 = 2.5;
// el primer mundo q se elige como destino (bodies[0] es el sol)
const FIRST_WORLD: usize = 1;

// el allow se quita cuando el hud (d3) y las siguientes tareas lean los campos q faltan
#[allow(dead_code)]
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
    // si shift venia apretado el cuadro pasado, pa saber cuando se aprieta de nuevo
    boost_was_down: bool,
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
            boost_was_down: false,
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

/// si el cuerpo mas cercano esta lo bastante cerca pa enganchar la orbita, el hud lo usa tambien
pub fn orbit_in_range(state: &GameState, scene: &Scene) -> bool {
    match state.nearest {
        Some(i) => state.nearest_dist < ORBIT_RANGE_RADII * scene.bodies[i].radius,
        None => false,
    }
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

    // paso 4: la orbita, en modo foto no se toca porque la nave esta congelada
    // shift solo suelta cuando se aprieta de nuevo, si no al enganchar con shift abajo se soltaria solo
    let boost_pressed = input.boost && !state.boost_was_down;
    state.boost_was_down = input.boost;
    if !state.photo_mode && !ship.warping() {
        if ship.orbit_body().is_some() {
            if input.orbit_toggle || boost_pressed {
                ship.release_orbit();
            }
        } else if input.orbit_toggle && orbit_in_range(state, scene) {
            if let Some(i) = nearest {
                ship.engage_orbit(i, scene, t);
            }
        }
    }

    // paso 5: tab arranca el viaje al destino, y si ya vamos viajando lo cancela
    if input.warp && !state.photo_mode {
        if ship.warping() {
            ship.cancel_warp();
        } else {
            ship.start_warp(state.warp_target, scene, t);
        }
    }
    state.orbit_lock = ship.orbit_body();
    state.warp = ship.warp_effect();

    // paso 6: con P entramos o salimos del modo foto
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
    fn la_f_solo_engancha_cerca_y_la_f_o_shift_sueltan() {
        let scene = Scene::new();
        let mut state = GameState::new();
        let press_f = ShipInput { orbit_toggle: true, ..Default::default() };
        let hold_shift = ShipInput { boost: true, ..Default::default() };
        let radius = scene.bodies[1].radius;
        let tick = |state: &mut GameState, ship: &mut Ship, input: &ShipInput| update(state, ship, &scene, input, 0.0, 0.016);

        // lejos no engancha
        let mut ship = ship_above(&scene, 1, radius * 2.6);
        tick(&mut state, &mut ship, &press_f);
        assert_eq!(state.orbit_lock, None);

        // cerca si, y otra f suelta
        let mut ship = ship_above(&scene, 1, radius * 2.0);
        tick(&mut state, &mut ship, &press_f);
        assert_eq!(state.orbit_lock, Some(1));
        tick(&mut state, &mut ship, &press_f);
        assert_eq!(state.orbit_lock, None);

        // shift tambien suelta
        tick(&mut state, &mut ship, &press_f);
        assert_eq!(state.orbit_lock, Some(1));
        tick(&mut state, &mut ship, &hold_shift);
        assert_eq!(state.orbit_lock, None);

        // si enganchamos con shift ya abajo no se suelta sola, tiene q apretarlo de nuevo
        let f_with_shift = ShipInput { orbit_toggle: true, boost: true, ..Default::default() };
        tick(&mut state, &mut ship, &f_with_shift);
        tick(&mut state, &mut ship, &hold_shift);
        assert_eq!(state.orbit_lock, Some(1));
        tick(&mut state, &mut ship, &ShipInput::default());
        tick(&mut state, &mut ship, &hold_shift);
        assert_eq!(state.orbit_lock, None);
    }

    #[test]
    fn tab_arranca_y_cancela_el_viaje() {
        let scene = Scene::new();
        let mut state = GameState::new();
        let press_tab = ShipInput { warp: true, ..Default::default() };
        let press_f = ShipInput { orbit_toggle: true, ..Default::default() };
        let mut ship = ship_above(&scene, 3, 5.0);
        let tick = |state: &mut GameState, ship: &mut Ship, input: &ShipInput| update(state, ship, &scene, input, 0.0, 0.016);

        // el destino por defecto es themyscira y gotham esta lejos
        tick(&mut state, &mut ship, &press_tab);
        assert!(ship.warping());

        // viajando la f no engancha nada
        tick(&mut state, &mut ship, &press_f);
        assert_eq!(state.orbit_lock, None);

        tick(&mut state, &mut ship, &press_tab);
        assert!(!ship.warping());
    }

    #[test]
    fn despues_de_llegar_da_tiempo_de_apretar_la_f() {
        let scene = Scene::new();
        let dt = 1.0 / 60.0;
        let mut shortest = f32::MAX;

        for to in 1..=5 {
            let from = if to == 5 { 1 } else { 5 };
            let mut state = GameState::new();
            state.warp_target = to;
            let mut ship = ship_above(&scene, from, 5.0);
            let mut t = 0.0;
            update(&mut state, &mut ship, &scene, &ShipInput { warp: true, ..Default::default() }, t, dt);
            while ship.warping() {
                t += dt;
                ship.update(ShipInput::default(), dt, &scene, t);
            }

            // ya llegamos y no se toca nada: cuanto tiempo sigue disponible la orbita
            update(&mut state, &mut ship, &scene, &ShipInput::default(), t, dt);
            assert_eq!(state.nearest, Some(to));
            assert!(orbit_in_range(&state, &scene));
            let mut in_range = 0.0;
            while orbit_in_range(&state, &scene) && in_range < 30.0 {
                t += dt;
                in_range += dt;
                ship.update(ShipInput::default(), dt, &scene, t);
                update(&mut state, &mut ship, &scene, &ShipInput::default(), t, dt);
            }
            shortest = shortest.min(in_range);
        }
        println!("despues de llegar la f sigue disponible al menos {shortest:.1} s");
        assert!(shortest > 3.0);
    }

    #[test]
    fn tab_desde_una_orbita_la_suelta_y_viaja() {
        let scene = Scene::new();
        let mut state = GameState::new();
        let mut ship = ship_above(&scene, 3, 5.0);
        update(&mut state, &mut ship, &scene, &ShipInput { orbit_toggle: true, ..Default::default() }, 0.0, 0.016);
        assert_eq!(state.orbit_lock, Some(3));

        update(&mut state, &mut ship, &scene, &ShipInput { warp: true, ..Default::default() }, 0.0, 0.016);
        assert_eq!(state.orbit_lock, None);
        assert!(ship.warping());
    }

    #[test]
    fn en_modo_foto_tab_no_hace_nada() {
        let scene = Scene::new();
        let mut state = GameState::new();
        state.photo_mode = true;
        let mut ship = ship_above(&scene, 3, 5.0);
        update(&mut state, &mut ship, &scene, &ShipInput { warp: true, ..Default::default() }, 0.0, 0.016);
        assert!(!ship.warping());
    }

    #[test]
    fn en_modo_foto_la_f_no_hace_nada() {
        let scene = Scene::new();
        let mut state = GameState::new();
        state.photo_mode = true;
        let mut ship = ship_above(&scene, 1, 2.0);
        update(&mut state, &mut ship, &scene, &ShipInput { orbit_toggle: true, ..Default::default() }, 0.0, 0.016);
        assert_eq!(state.orbit_lock, None);
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
