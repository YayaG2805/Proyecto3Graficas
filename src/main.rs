//! Proyecto final Gráficas por Computadora — Sistema solar con renderer por software.
//!
//! raylib SOLO se usa para la ventana, leer teclado/mouse, mostrar nuestro framebuffer
//! como textura 2D y escribir texto del HUD. Todo el 3D lo hace `renderer.rs`.

mod creatures;
mod exotics;
mod hud;
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

use creatures::{create_creatures, Creature};
use exotics::{AsteroidBelt, Comet, ASTEROID_SEED};
use hud::{Canvas, RadarBlip};
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

// --- HUD ---
/// Segundos que dura abierta la ficha de un planeta escaneado.
const SCAN_CARD_SECONDS: f32 = 6.0;
/// La ficha se cierra si la nave se aleja más de estos radios de la superficie.
const SCAN_CARD_CLOSE_RADII: f32 = 4.0;
/// Se muestra el aviso "[E] escanear" a menos de estos radios (igual que el rango de interaction.rs).
const SCAN_HINT_RADII: f32 = 3.0;
/// Rectángulo de la ficha en píxeles INTERNOS (la ventana es el doble).
const CARD_X0: i32 = 440;
const CARD_Y0: i32 = 44;
const CARD_X1: i32 = 634;
const CARD_Y1: i32 = 152;
/// Letras por renglón del dato curioso.
const CARD_WRAP: usize = 40;
/// Escala de la resolución interna a la ventana (1280 / 640).
const HUD_SCALE: i32 = WINDOW_W / RENDER_W as i32;

/// Color del punto de cada cuerpo en el radar.
fn body_color(name: &str) -> Vec3 {
    match name {
        "Sol" => Vec3::new(1.0, 0.8, 0.2),
        "Themyscira" => Vec3::new(0.45, 0.85, 0.55),
        "Krypton" => Vec3::new(0.40, 0.70, 1.0),
        "Gotham" => Vec3::new(0.62, 0.62, 0.68),
        "Atlantis" => Vec3::new(0.15, 0.45, 1.0),
        "Oa" => Vec3::new(0.30, 1.0, 0.40),
        "Central City" => Vec3::new(1.0, 0.92, 0.30),
        _ => Vec3::new(0.9, 0.9, 0.9),
    }
}

/// Todo lo que necesitamos para dibujar un cuadro.
struct Game {
    renderer: Renderer,
    scene: Scene,
    /// Cinturón de asteroides entre Krypton y Gotham (exotics.rs).
    asteroids: AsteroidBelt,
    /// Cometa en órbita elíptica (exotics.rs).
    comet: Comet,
    /// Personajes de la Liga que viven en los mundos (creatures.rs).
    creatures: Vec<Creature>,
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
    /// Cuerpo cuya ficha está abierta en el HUD y cuántos segundos lleva abierta.
    card_body: Option<usize>,
    card_timer: f32,
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
        // los cristales se reparten alrededor de los mundos, por eso el estado necesita la escena
        let scene = Scene::new();
        let state = GameState::new(&scene);
        let creatures = create_creatures(&scene);
        Game {
            renderer: Renderer::new(RENDER_W, RENDER_H, uniforms),
            scene,
            asteroids: AsteroidBelt::new(ASTEROID_SEED),
            comet: Comet::new(),
            creatures,
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
            state,
            card_body: None,
            card_timer: 0.0,
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

        // paso 3b: la ficha del planeta escaneado se cierra sola a los SCAN_CARD_SECONDS
        // o si la nave se aleja. Usa dt real, así también corre en modo foto.
        if self.state.scanned != self.card_body {
            self.card_body = self.state.scanned; // escaneo nuevo: reinicia el reloj
            self.card_timer = 0.0;
        }
        if let Some(i) = self.card_body {
            self.card_timer += dt;
            let radius = self.scene.bodies[i].radius;
            let dist = (self.scene.body_position(i, self.time) - self.ship.position).length() - radius;
            if self.card_timer > SCAN_CARD_SECONDS || dist > SCAN_CARD_CLOSE_RADII * radius {
                self.state.scanned = None;
                self.card_body = None;
            }
        }

        // paso 4: la camara va de ultimo pa q siga a la nave ya en su posicion final del cuadro
        self.camera.update(&self.ship, dt);
    }

    fn render(&mut self) {
        self.draw_geometry();
        self.shade();
        self.draw_overlay();
    }

    /// HUD dibujado en NUESTRO framebuffer: el radar y el fondo de la ficha.
    /// Va después del sombreado porque se pinta encima de la imagen final.
    fn draw_overlay(&mut self) {
        let mut blips: Vec<RadarBlip> = self
            .scene
            .bodies
            .iter()
            .enumerate()
            .map(|(i, b)| RadarBlip {
                offset: self.scene.body_position(i, self.time) - self.ship.position,
                size: (b.radius * 0.35).clamp(1.5, 4.0),
                color: body_color(b.name),
                highlight: i == self.state.warp_target,
            })
            .collect();
        // El cometa también sale en el radar, como un punto celeste pequeño.
        blips.push(RadarBlip {
            offset: self.comet.position(self.time) - self.ship.position,
            size: 1.5,
            color: Vec3::new(0.6, 0.9, 1.0),
            highlight: false,
        });
        let mut cv = Canvas { px: &mut self.renderer.color, w: RENDER_W, h: RENDER_H };
        hud::draw_radar(&mut cv, self.ship.forward(), &blips);
        if self.card_body.is_some() {
            hud::draw_panel(&mut cv, CARD_X0, CARD_Y0, CARD_X1, CARD_Y1);
        }
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
        self.state.draw_collectibles(&mut self.renderer, &self.scene, self.time);
        self.asteroids.draw(&mut self.renderer, self.time);
        self.comet.draw_nucleus(&mut self.renderer, self.time);
        creatures::draw_creatures(&mut self.renderer, &self.scene, self.time, &self.creatures);
        let model = self.ship.model_matrix(1.0);
        // El shader de la nave recibe el estado del motor en cada cuadro (sus "uniforms" propios).
        let ship_shader: Arc<dyn FragmentShader> = if self.ship_is_halcon {
            Arc::new(HalconShader { engine_glow: self.ship.engine_glow() })
        } else {
            Arc::new(ShipShader { base: SHIP_COLOR, boost: self.ship.boost })
        };
        self.renderer.draw_mesh(&self.ship_mesh, &model, ship_shader);

        // Lo aditivo (la cola y el halo del cometa, la llama del turbo) va SIEMPRE al final,
        // porque necesita el z-buffer completo de lo opaco.
        self.comet.draw_glow(&mut self.renderer, self.time);

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
    // La luna Central City se fotografía en un momento con un rayo cruzándola.
    let cc_time = 1.1;
    let cc = g.scene.bodies.iter().position(|b| b.name == "Central City").unwrap_or(1);
    let cc_pos = g.scene.body_position(cc, cc_time);
    // (archivo, posición de la nave, cuerpo al que apunta, mapeo de normales, warp, turbo, tiempo)
    let shots = [
        ("snap_lejos.ppm", Vec3::new(0.0, 25.0, 95.0), 1, true, 0.0, 0.0, 0.0),
        ("snap_cerca.ppm", Vec3::new(45.0, 4.0, 14.0), 1, true, 0.0, 0.0, 0.0),
        ("snap_superficie.ppm", Vec3::new(46.5, 4.6, 2.5), 1, true, 0.0, 0.0, 0.0),
        // Misma vista sin mapeo de normales, para comparar.
        ("snap_superficie_sin_normales.ppm", Vec3::new(46.5, 4.6, 2.5), 1, false, 0.0, 0.0, 0.0),
        // Efecto warp al máximo.
        ("snap_warp.ppm", Vec3::new(0.0, 25.0, 95.0), 1, true, 1.0, 0.0, 0.0),
        // Turbo al máximo: motores blancos + llama.
        ("snap_turbo.ppm", Vec3::new(0.0, 25.0, 95.0), 1, true, 0.0, 1.0, 0.0),
        // Luna Central City con Gotham detrás (jerarquía de transformaciones).
        ("snap_central_city.ppm", cc_pos + Vec3::new(1.6, 0.6, 2.2), cc, true, 0.0, 0.0, cc_time),
        // Dentro del cinturón de asteroides, mirando al sol.
        ("snap_asteroides.ppm", BELT_SHOT, 0, true, 0.0, 0.0, 0.0),
    ];
    for (name, pos, aim, normal_mapping, warp, boost, time) in shots {
        g.time = time;
        g.normal_mapping = normal_mapping;
        g.warp = warp;
        g.ship.boost = boost;
        aim_ship(&mut g, pos, aim);
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
        g.draw_overlay(); // radar del HUD, para verlo también en las capturas
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

    // Cometa visto de lado, para que se note que la cola apunta lejos del sol.
    g.time = 0.0;
    g.ship.boost = 0.0;
    // Un retrato de cada personaje de la Liga: la cámara se pone a mano cerca de él (algunos son
    // muy pequeños para la cámara de persecución) y la nave se manda detrás de la cámara.
    for i in 0..g.creatures.len() {
        let (name, hero, planet, size) = {
            let c = &g.creatures[i];
            (c.name, c.position(&g.scene, g.time), g.scene.body_position(c.home, g.time), c.size)
        };
        let out = (hero - planet).normalized();
        let side = out.cross(Vec3::UP).normalized();
        let eye = hero + (out * 0.6 + side * 0.8 + Vec3::UP * 0.3) * (size * 8.0);
        g.ship.position = eye + (eye - hero).normalized() * 30.0;
        g.camera.position = eye;
        g.camera.target = hero;
        g.camera.up = Vec3::UP;
        g.render();
        let file = format!("snap_{}.ppm", name.to_lowercase().replace(' ', "_"));
        g.renderer.save_ppm(&file).expect("no se pudo guardar");
        println!("{file}: listo");
    }

    let comet = g.comet.position(g.time);
    let away = comet.normalized();
    let side = away.cross(Vec3::UP).normalized();
    aim_ship_at(&mut g, comet + side * 28.0 + away * 6.0, comet + away * 9.0);
    g.render();
    g.renderer.save_ppm("snap_cometa.ppm").expect("no se pudo guardar");
    println!("snap_cometa.ppm: cometa a {:.0} del sol", comet.length());

    // Agujero negro lejano (en el cielo): se apunta un poco abajo para que la nave no lo tape.
    let from = Vec3::new(0.0, 25.0, 95.0);
    aim_ship_at(&mut g, from, from + shaders::BH_DIR.normalized() * 100.0 - Vec3::UP * 22.0);
    g.render();
    g.renderer.save_ppm("snap_agujero_negro.ppm").expect("no se pudo guardar");
    println!("snap_agujero_negro.ppm: listo");

    // Comparación para la demo: la vista del cinturón con culling (C) y LOD (L) prendidos y apagados.
    // Demuestra que las dos técnicas cambian el trabajo que hace el renderer.
    aim_ship(&mut g, BELT_SHOT, 0);
    for (culling, lod) in [(true, true), (false, true), (true, false), (false, false)] {
        g.renderer.frustum_culling = culling;
        g.renderer.use_lod = lod;
        let start = std::time::Instant::now();
        for _ in 0..10 {
            g.render();
        }
        let ms = start.elapsed().as_secs_f32() * 1000.0 / 10.0;
        let s = g.renderer.stats;
        println!(
            "cinturon C={} L={}: {ms:.1} ms/cuadro  dibujados={} descartados={} tri_in={}",
            if culling { "ON " } else { "OFF" },
            if lod { "ON " } else { "OFF" },
            s.objects_drawn,
            s.objects_culled,
            s.triangles_in
        );
    }
}

/// Posición de la nave para las tomas del cinturón de asteroides.
const BELT_SHOT: Vec3 = Vec3::new(0.0, 1.5, 88.0);

/// Pone la nave en `pos` mirando al cuerpo `aim`, con la cámara ya detrás de ella (para el snapshot).
fn aim_ship(g: &mut Game, pos: Vec3, aim: usize) {
    let target = g.scene.body_position(aim, g.time);
    aim_ship_at(g, pos, target);
}

/// Pone la nave en `pos` mirando al punto `target`, con la cámara ya detrás de ella.
fn aim_ship_at(g: &mut Game, pos: Vec3, target: Vec3) {
    g.ship.position = pos;
    let d = (target - pos).normalized();
    g.ship.yaw = (-d.x).atan2(-d.z);
    g.ship.pitch = d.y.asin();
    g.camera = ChaseCamera::new(&g.ship);
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
        // El destino del warp, si ya vamos viajando y los coleccionables
        // (el total lo llena interaction.rs cuando haya cristales).
        let target = game.scene.bodies[game.state.warp_target].name;
        let crystals = if game.state.total > 0 {
            format!("{}/{}", game.state.collected, game.state.total)
        } else {
            "-".to_string()
        };
        d.draw_text(
            &format!(
                "destino [1-5]: {target}   warp [Tab]: {}   cristales: {crystals}",
                if game.ship.warping() { "viajando" } else { "listo" }
            ),
            10,
            78,
            18,
            Color::SKYBLUE,
        );

        // Aviso para escanear cuando hay un mundo cerca y su ficha no está abierta.
        if let Some(i) = game.state.nearest {
            let b = &game.scene.bodies[i];
            if game.card_body.is_none() && game.state.nearest_dist < SCAN_HINT_RADII * b.radius {
                d.draw_text(&format!("[E] escanear {}", hud::to_ascii(b.name)), 10, 102, 20, Color::GOLD);
            }
        }

        // Ficha del planeta escaneado (el fondo ya lo pintó draw_overlay en el framebuffer).
        if let Some(i) = game.card_body {
            let b = &game.scene.bodies[i];
            let (x, mut y) = (CARD_X0 * HUD_SCALE + 14, CARD_Y0 * HUD_SCALE + 12);
            d.draw_text(&hud::to_ascii(b.info.title), x, y, 24, Color::RAYWHITE);
            y += 30;
            d.draw_text(&format!("Heroe: {}", hud::to_ascii(b.info.hero)), x, y, 18, Color::GOLD);
            y += 28;
            for line in hud::wrap(&hud::to_ascii(b.info.fact), CARD_WRAP) {
                d.draw_text(&line, x, y, 16, Color::LIGHTGRAY);
                y += 20;
            }
            d.draw_text(
                &format!("radio: {:.1}   gravedad: {:.0}", b.radius, b.gravity),
                x,
                CARD_Y1 * HUD_SCALE - 30,
                18,
                Color::SKYBLUE,
            );
        }

        if show_help {
            // son dos lineas porque en una ya no cabia en la ventana
            d.draw_text(
                "W/S acelerar/frenar   A/D girar   Flechas subir/bajar   Shift turbo   Espacio frenar   F orbita   E escanear",
                10,
                WINDOW_H - 48,
                16,
                Color::GOLD,
            );
            d.draw_text(
                "1-5 destino   Tab warp (otro Tab cancela)   G gravedad   P foto   C culling   L LOD   O orbitas   N normales   H ayuda",
                10,
                WINDOW_H - 26,
                16,
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
    fn un_cristal_recogido_ya_no_se_dibuja() {
        let mut g = Game::new();
        let dt = 1.0 / 60.0;
        // sin culling todo cuenta, asi la resta no depende de pa donde mire la camara
        g.renderer.frustum_culling = false;
        g.draw_geometry();
        let before = g.renderer.stats.objects_drawn;

        // pongo la nave encima de un cristal y corre un cuadro
        g.ship.position = g.state.remaining_positions(&g.scene, g.time + dt)[0];
        g.update(ShipInput::default(), dt);
        assert_eq!(g.state.collected, 1);

        g.draw_geometry();
        assert_eq!(g.renderer.stats.objects_drawn, before - 1);
    }

    #[test]
    fn el_efecto_warp_lo_prende_el_viaje() {
        let mut g = Game::new();
        let dt = 1.0 / 60.0;

        // sin viaje no hay efecto
        g.update(ShipInput::default(), dt);
        assert_eq!(g.warp, 0.0);

        // tab arranca el viaje, pero el efecto no prende hasta q termine de alinearse
        g.update(ShipInput { warp: true, ..Default::default() }, dt);
        assert!(g.ship.warping());
        assert_eq!(g.warp, 0.0);

        // a medio viaje el efecto llega alto, y lo q llega al render es lo del estado
        let mut strongest = 0.0f32;
        for _ in 0..300 {
            g.update(ShipInput::default(), dt);
            assert_eq!(g.warp, g.state.warp);
            strongest = strongest.max(g.warp);
        }
        assert!(strongest > 0.5);
    }
}
