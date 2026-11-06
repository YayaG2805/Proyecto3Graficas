//! Proyecto final Gráficas por Computadora — Sistema solar con renderer por software.
//!
//! raylib SOLO se usa para la ventana, leer teclado/mouse, mostrar nuestro framebuffer
//! como textura 2D y escribir texto del HUD. Todo el 3D lo hace `renderer.rs`.

mod interaction;
mod math;
mod mesh;
mod noise;
mod normal_map;
mod props;
mod renderer;
mod scene;
mod shaders;
mod ship;
mod texture;

use interaction::GameState;
use math::{Mat4, Vec3};
use mesh::Mesh;
use raylib::prelude::*;
use renderer::{FragmentShader, Renderer, Uniforms};
use scene::Scene;
use shaders::{ExhaustShader, ShipShader, Sky};
use ship::{ChaseCamera, HalconShader, Ship, ShipInput};
use std::sync::Arc;

/// Resolución INTERNA del renderer (se escala a la ventana). Bajarla = más FPS.
const RENDER_W: usize = 640;
const RENDER_H: usize = 360;
const WINDOW_W: i32 = 1280;
const WINDOW_H: i32 = 720;

const FOV_Y: f32 = 60.0 * std::f32::consts::PI / 180.0;
/// Cuánto se abre el campo de visión con el warp al máximo (sensación de velocidad).
const WARP_FOV_EXTRA: f32 = 30.0 * std::f32::consts::PI / 180.0;
const NEAR: f32 = 0.1;
const FAR: f32 = 2000.0;
/// Color de la nave (gris claro de carguero).
const SHIP_COLOR: Vec3 = Vec3::new(0.72, 0.72, 0.70);
/// Qué fracción trasera del largo de la nave se usa para medir de dónde sale la llama del turbo.
const EXHAUST_ZONE: f32 = 0.05;

/// Todo lo que necesitamos para dibujar un cuadro.
struct Game {
    renderer: Renderer,
    scene: Scene,
    ship: Ship,
    ship_mesh: Mesh,
    /// true si cargo el halcon, false si estamos con el modelo de respaldo
    ship_is_halcon: bool,
    /// Llama del turbo y el punto (en espacio de la nave) de donde sale.
    exhaust_mesh: Mesh,
    exhaust_origin: Vec3,
    /// Cielo con la nebulosa precalculada.
    sky: Sky,
    camera: ChaseCamera,
    time: f32,
    /// Mapeo de normales encendido (tecla N).
    normal_mapping: bool,
    /// Efecto visual del warp, de 0 a 1. Viene de `state.warp` en cada cuadro.
    warp: f32,
    /// cual mundo esta cerca, el destino, el modo foto... lo actualiza interaction.rs
    state: GameState,
}

impl Game {
    fn new() -> Game {
        // Halcón Milenario modelado en Blender. La nave anterior sigue en assets/nave.obj.
        // si no esta caemos a la nave vieja y si tampoco a la provisional
        let (ship_mesh, ship_is_halcon) = match Mesh::load_obj("assets/Halcon_Milenario.obj", Some(1.5)) {
            Ok(mesh) => (mesh, true),
            Err(e) => {
                eprintln!("No se pudo cargar el halcon ({e}); probando con nave.obj");
                let mesh = Mesh::load_obj("assets/nave.obj", Some(1.5)).unwrap_or_else(|e| {
                    eprintln!("No se pudo cargar la nave ({e}); usando nave provisional");
                    Mesh::placeholder_ship()
                });
                (mesh, false)
            }
        };
        // La llama sale de la parte más trasera de la nave (local +Z = atrás): tomamos los
        // vértices que están en el último EXHAUST_ZONE del largo y medimos su ancho y alto.
        let (min_z, max_z) = ship_mesh
            .vertices
            .iter()
            .fold((f32::MAX, f32::MIN), |(a, b), v| (a.min(v.position.z), b.max(v.position.z)));
        let rear_z = max_z - (max_z - min_z) * EXHAUST_ZONE;
        let (lo, hi) = ship_mesh
            .bounds_where(|v| v.position.z >= rear_z)
            .unwrap_or((Vec3::new(-0.5, -0.05, 0.8), Vec3::new(0.5, 0.05, 1.0)));
        let half_w = (hi.x - lo.x) * 0.5 * 0.9;
        let half_h = ((hi.y - lo.y) * 0.5 * 1.6).max(half_w * 0.18);
        // Empieza un poco metida en la nave: lo que queda dentro del casco lo tapa el
        // z-buffer, así no hay huecos entre la nave y la llama.
        let exhaust_origin = Vec3::new((lo.x + hi.x) * 0.5, (lo.y + hi.y) * 0.5, (lo.z + hi.z) * 0.5);
        let exhaust_mesh = Mesh::exhaust_plume(half_w, half_h, 24);

        let ship = Ship::new(Vec3::new(0.0, 3.0, 70.0));
        let camera = ChaseCamera::new(&ship);
        let uniforms = Game::make_uniforms(&camera, 0.0, true, 0.0, ship.position);
        Game {
            renderer: Renderer::new(RENDER_W, RENDER_H, uniforms),
            scene: Scene::new(),
            ship,
            ship_mesh,
            ship_is_halcon,
            exhaust_mesh,
            exhaust_origin,
            sky: Sky::new(),
            camera,
            time: 0.0,
            normal_mapping: true,
            warp: 0.0,
            state: GameState::new(),
        }
    }

    fn make_uniforms(camera: &ChaseCamera, time: f32, normal_mapping: bool, warp: f32, ship_pos: Vec3) -> Uniforms {
        // Con warp el campo de visión se abre: los bordes "se estiran" y parece que vamos más rápido.
        let fov_y = FOV_Y + warp * WARP_FOV_EXTRA;
        let (r, u, f) = camera.basis();
        let aspect = RENDER_W as f32 / RENDER_H as f32;
        Uniforms {
            time,
            view: camera.view(),
            projection: Mat4::perspective(fov_y, aspect, NEAR, FAR),
            camera_pos: camera.position,
            ship_pos,
            camera_right: r,
            camera_up: u,
            camera_forward: f,
            fov_y,
            aspect,
            near: NEAR,
            far: FAR,
            light_pos: Vec3::ZERO, // el sol está en el origen
            normal_mapping,
            warp,
        }
    }

    fn update(&mut self, input: ShipInput, dt: f32) {
        // paso 1: en modo foto todo se queda congelado, ni el tiempo ni la nave avanzan
        // como el tiempo no corre, los planetas y los shaders animados tambien se quedan quietos
        if !self.state.photo_mode {
            self.time += dt;
            self.ship.update(input, dt, &self.scene, self.time);
        }

        // paso 2: esto corre siempre, aun en modo foto, si no la p nunca podria apagarlo
        interaction::update(&mut self.state, &mut self.ship, &self.scene, &input, self.time, dt);

        // paso 3: el efecto del warp ahora lo decide el estado del juego
        self.warp = self.state.warp;

        // paso 4: la camara va de ultimo pa q siga a la nave ya en su posicion final del cuadro
        self.camera.update(&self.ship, dt);
    }

    fn render(&mut self) {
        self.draw_geometry();
        self.shade();
    }

    /// Etapa 8: fragment shaders de cada píxel visible; los vacíos usan el cielo.
    fn shade(&mut self) {
        let sky = &self.sky;
        self.renderer.finish_frame(&|dir, u| sky.color(dir, u));
    }

    /// Etapas 1 a 7 (vértices, recorte, rasterizado, z-buffer). El sombreado va aparte.
    fn draw_geometry(&mut self) {
        let u = Game::make_uniforms(&self.camera, self.time, self.normal_mapping, self.warp, self.ship.position);
        self.renderer.begin_frame(u);
        self.scene.draw(&mut self.renderer, self.time);
        let model = self.ship.model_matrix(1.0);
        // El shader de la nave recibe el estado del motor en cada cuadro (sus "uniforms" propios).
        let ship_shader: Arc<dyn FragmentShader> = if self.ship_is_halcon {
            Arc::new(HalconShader { engine_glow: self.ship.engine_glow() })
        } else {
            Arc::new(ShipShader { base: SHIP_COLOR, boost: self.ship.boost })
        };
        self.renderer.draw_mesh(&self.ship_mesh, &model, ship_shader);

        // Llama del turbo: SIEMPRE al final, porque es aditiva y necesita el z-buffer completo.
        if self.ship.boost > 0.01 {
            // Largo de la llama (en unidades de la nave, que mide ~3 de punta a punta).
            let length = 0.4 + 2.4 * self.ship.boost;
            let plume = model * Mat4::translation(self.exhaust_origin) * Mat4::scale(Vec3::new(1.0, 1.0, length));
            self.renderer
                .draw_mesh_additive(&self.exhaust_mesh, &plume, Arc::new(ExhaustShader { boost: self.ship.boost }));
        }
    }
}

/// Modo sin ventana: `cargo run --release -- --snapshot` guarda capturas PPM.
/// Sirve para probar el renderer sin abrir la ventana.
fn snapshot() {
    let mut g = Game::new();
    // (archivo, posición de la nave, mapeo de normales, warp, turbo)
    let shots = [
        ("snap_lejos.ppm", Vec3::new(0.0, 25.0, 95.0), true, 0.0, 0.0),
        ("snap_cerca.ppm", Vec3::new(45.0, 4.0, 14.0), true, 0.0, 0.0),
        ("snap_superficie.ppm", Vec3::new(46.5, 4.6, 2.5), true, 0.0, 0.0),
        // Misma vista sin mapeo de normales, para comparar.
        ("snap_superficie_sin_normales.ppm", Vec3::new(46.5, 4.6, 2.5), false, 0.0, 0.0),
        // Efecto warp al máximo.
        ("snap_warp.ppm", Vec3::new(0.0, 25.0, 95.0), true, 1.0, 0.0),
        // Turbo al máximo: motores blancos + llama.
        ("snap_turbo.ppm", Vec3::new(0.0, 25.0, 95.0), true, 0.0, 1.0),
    ];
    for (name, pos, normal_mapping, warp, boost) in shots {
        g.normal_mapping = normal_mapping;
        g.warp = warp;
        g.ship.boost = boost;
        g.ship.position = pos;
        // Apuntar la nave hacia el planeta rocoso.
        let target = g.scene.body_position(1, g.time);
        let d = (target - pos).normalized();
        g.ship.yaw = (-d.x).atan2(-d.z);
        g.ship.pitch = d.y.asin();
        g.camera = ChaseCamera::new(&g.ship);
        // Medir rendimiento: promedio de 30 cuadros, separando geometría y sombreado.
        let (mut geo, mut shade) = (0.0f32, 0.0f32);
        for _ in 0..30 {
            let t0 = std::time::Instant::now();
            g.draw_geometry();
            let t1 = std::time::Instant::now();
            g.shade();
            geo += (t1 - t0).as_secs_f32();
            shade += t1.elapsed().as_secs_f32();
        }
        let (geo, shade) = (geo * 1000.0 / 30.0, shade * 1000.0 / 30.0);
        g.renderer.save_ppm(name).expect("no se pudo guardar");
        let s = g.renderer.stats;
        println!(
            "{name}: {:.1} ms/cuadro (geometria {geo:.1} + sombreado {shade:.1})  dibujados={} descartados={} tri_in={} tri_raster={}",
            geo + shade,
            s.objects_drawn,
            s.objects_culled,
            s.triangles_in,
            s.triangles_rasterized
        );
    }
}

fn read_input(rl: &RaylibHandle) -> ShipInput {
    let axis = |pos: KeyboardKey, neg: KeyboardKey| (rl.is_key_down(pos) as i32 - rl.is_key_down(neg) as i32) as f32;
    ShipInput {
        thrust: axis(KeyboardKey::KEY_W, KeyboardKey::KEY_S),
        turn: axis(KeyboardKey::KEY_D, KeyboardKey::KEY_A),
        climb: axis(KeyboardKey::KEY_UP, KeyboardKey::KEY_DOWN),
        boost: rl.is_key_down(KeyboardKey::KEY_LEFT_SHIFT),
        brake: rl.is_key_down(KeyboardKey::KEY_SPACE),
        // las de un toque van con is_key_pressed, asi no se repiten mientras la tecla sigue abajo
        orbit_toggle: rl.is_key_pressed(KeyboardKey::KEY_F),
        scan: rl.is_key_pressed(KeyboardKey::KEY_E),
        warp: rl.is_key_pressed(KeyboardKey::KEY_TAB),
        select: read_world_key(rl),
        photo: rl.is_key_pressed(KeyboardKey::KEY_P),
        gravity_toggle: rl.is_key_pressed(KeyboardKey::KEY_G),
    }
}

// revisa las teclas 1 a 5 y devuelve el numero de la q se acaba de apretar
fn read_world_key(rl: &RaylibHandle) -> Option<usize> {
    let keys = [
        KeyboardKey::KEY_ONE,
        KeyboardKey::KEY_TWO,
        KeyboardKey::KEY_THREE,
        KeyboardKey::KEY_FOUR,
        KeyboardKey::KEY_FIVE,
    ];
    // la tecla 1 esta en la posicion 0 de la lista, por eso el + 1
    keys.iter().position(|&key| rl.is_key_pressed(key)).map(|i| i + 1)
}

fn main() {
    if std::env::args().any(|a| a == "--snapshot") {
        snapshot();
        return;
    }

    let (mut rl, thread) = raylib::init().size(WINDOW_W, WINDOW_H).title("Sistema Solar - Software Renderer").build();
    rl.set_target_fps(60);

    let mut game = Game::new();

    // Textura donde se copia nuestro framebuffer cada cuadro.
    let image = Image::gen_image_color(RENDER_W as i32, RENDER_H as i32, Color::BLACK);
    let mut texture = rl.load_texture_from_image(&thread, &image).expect("no se pudo crear la textura");
    let mut show_help = true;

    while !rl.window_should_close() {
        // dt limitado para que un tirón (p.ej. al mover la ventana) no teletransporte la nave.
        let dt = rl.get_frame_time().min(0.05);

        if rl.is_key_pressed(KeyboardKey::KEY_C) {
            game.renderer.frustum_culling = !game.renderer.frustum_culling;
        }
        if rl.is_key_pressed(KeyboardKey::KEY_L) {
            game.renderer.use_lod = !game.renderer.use_lod;
        }
        if rl.is_key_pressed(KeyboardKey::KEY_O) {
            game.scene.show_orbits = !game.scene.show_orbits;
        }
        if rl.is_key_pressed(KeyboardKey::KEY_N) {
            game.normal_mapping = !game.normal_mapping;
        }
        if rl.is_key_pressed(KeyboardKey::KEY_H) {
            show_help = !show_help;
        }

        let input = read_input(&rl);
        game.update(input, dt);
        game.render();
        texture.update_texture(&game.renderer.color).expect("no se pudo actualizar la textura");

        let fps = rl.get_fps();
        let mut d = rl.begin_drawing(&thread);
        d.clear_background(Color::BLACK);
        d.draw_texture_pro(
            &texture,
            Rectangle::new(0.0, 0.0, RENDER_W as f32, RENDER_H as f32),
            Rectangle::new(0.0, 0.0, WINDOW_W as f32, WINDOW_H as f32),
            Vector2::zero(),
            0.0,
            Color::WHITE,
        );

        // HUD
        let s = game.renderer.stats;
        let speed = game.ship.velocity.length();
        // en orbita muestra el mundo, si no avisa si ya estamos lo bastante cerca pa apretar la f
        let orbit_text = match game.state.orbit_lock {
            Some(i) => game.scene.bodies[i].name,
            None if interaction::orbit_in_range(&game.state, &game.scene) => "disponible",
            None => "lejos",
        };
        d.draw_text(&format!("FPS: {fps}   velocidad: {speed:.1}   warp: {:.2}   orbita [F]: {orbit_text}", game.warp), 10, 10, 20, Color::RAYWHITE);
        d.draw_text(
            &format!(
                "objetos: {} dibujados / {} descartados   triangulos: {} entran / {} rasterizados",
                s.objects_drawn, s.objects_culled, s.triangles_in, s.triangles_rasterized
            ),
            10,
            34,
            18,
            Color::LIGHTGRAY,
        );
        d.draw_text(
            &format!(
                "culling [C]: {}   LOD [L]: {}   orbitas [O]: {}   normales [N]: {}   gravedad [G]: {}",
                if game.renderer.frustum_culling { "ON" } else { "OFF" },
                if game.renderer.use_lod { "ON" } else { "OFF" },
                if game.scene.show_orbits { "ON" } else { "OFF" },
                if game.normal_mapping { "ON" } else { "OFF" },
                if game.ship.gravity_on { "ON" } else { "OFF" }
            ),
            10,
            56,
            18,
            Color::LIGHTGRAY,
        );
        if show_help {
            d.draw_text(
                "W/S acelerar/frenar   A/D girar   Flechas subir/bajar   Shift turbo   Espacio frenar   O orbitas   N normales   F orbita   G gravedad   P foto   H ocultar ayuda",
                10,
                WINDOW_H - 28,
                18,
                Color::GOLD,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn el_modo_foto_congela_todo_menos_la_camara() {
        let mut g = Game::new();
        let dt = 1.0 / 60.0;
        let press_p = ShipInput { photo: true, ..Default::default() };
        let accelerate = ShipInput { thrust: 1.0, ..Default::default() };

        // volando normal el tiempo y la nave si avanzan
        g.update(accelerate, dt);
        assert!(g.time > 0.0);
        assert!(g.ship.velocity.length() > 0.0);

        // entramos a modo foto y guardamos como quedo todo
        g.update(press_p, dt);
        assert!(g.state.photo_mode);
        let (time, position) = (g.time, g.ship.position);

        // muevo la camara a otro lado: la nave y el tiempo no se mueven, pero la camara si regresa
        g.camera.position = position + Vec3::new(50.0, 0.0, 0.0);
        let camera_before = g.camera.position;
        for _ in 0..30 {
            g.update(accelerate, dt);
        }
        assert_eq!(g.time, time);
        assert_eq!((g.ship.position - position).length(), 0.0);
        assert!((g.camera.position - camera_before).length() > 0.0);

        // con otra p sale del modo foto y todo sigue
        g.update(press_p, dt);
        assert!(!g.state.photo_mode);
        g.update(accelerate, dt);
        assert!(g.time > time);
    }

    #[test]
    fn el_efecto_warp_viene_del_estado_y_no_de_la_tecla() {
        let mut g = Game::new();
        let dt = 1.0 / 60.0;

        // apretar tab solo ya no prende el efecto
        g.update(ShipInput { warp: true, ..Default::default() }, dt);
        assert_eq!(g.warp, 0.0);

        // lo q mande es lo q tenga el estado
        g.state.warp = 0.7;
        g.update(ShipInput::default(), dt);
        assert_eq!(g.warp, 0.7);
    }
}
