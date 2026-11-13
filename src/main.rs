//! Proyecto final Gráficas por Computadora — Sistema solar con renderer por software.
//!
//! raylib SOLO se usa para la ventana, leer teclado/mouse, mostrar nuestro framebuffer
//! como textura 2D y escribir texto del HUD. Todo el 3D lo hace `renderer.rs`.
//! También reproduce los .wav del sonido; qué suena y cuándo lo decide `audio.rs`.

mod audio;
mod apokolips;
mod creatures;
mod exotics;
mod explosion;
mod hud;
mod interaction;
mod math;
mod mesh;
mod mundos;
mod noise;
mod normal_map;
mod oa;
mod props;
mod renderer;
mod scene;
mod shaders;
mod ship;
mod texture;
mod tour;

use creatures::{create_creatures, Creature};
use exotics::{AsteroidBelt, Comet, ASTEROID_SEED};
use explosion::{Explosion, ExplosionAssets, EXPLOSION_TIME, FLASH_TIME, SHAKE_STRENGTH, SHAKE_TIME};
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
/// Arrancar en pantalla completa (sin bordes, a la resolución del monitor). F11 la quita o la pone.
const START_FULLSCREEN: bool = true;

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

// --- Calor del Sol y explosión ---
/// El aviso de calor empieza a esta distancia de la colisión del Sol y llega al máximo al tocarla.
const HEAT_RANGE: f32 = 18.0;
/// Desde qué calor sale el letrero de alerta, y desde cuál parpadea.
const HEAT_WARNING: f32 = 0.25;
const HEAT_DANGER: f32 = 0.6;
/// Temblor de la cámara con el calor al máximo (unidades del mundo).
const HEAT_SHAKE: f32 = 0.08;
const HEAT_COLOR: Vec3 = Vec3::new(1.0, 0.3, 0.05);
const FLASH_COLOR: Vec3 = Vec3::new(1.0, 0.95, 0.8);

// --- Pantalla de inicio: la cámara gira despacio alrededor de todo el sistema ---
const TITLE_DISTANCE: f32 = 210.0;
const TITLE_HEIGHT: f32 = 60.0;
/// Radianes por segundo de la vuelta de la cámara.
const TITLE_SPIN: f32 = 0.04;
/// Oscurece el centro de la pantalla para que el título se lea sobre los planetas.
const TITLE_SHADE: f32 = 0.45;

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
        "Apokolips" => Vec3::new(1.0, 0.3, 0.1),
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
    /// Batería, torres, anillo, patrulla y auroras de Oa (oa.rs).
    oa: Option<oa::Oa>,
    /// Monumentos y detalles de cada mundo (carpeta mundos/).
    mundos: mundos::Mundos,
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
    /// Índice del Sol (el único cuerpo que no orbita): tocarlo destruye la nave.
    sun: Option<usize>,
    /// Qué tan cerca va la nave del Sol: 0 = lejos, 1 = tocándolo (aviso de calor).
    heat: f32,
    /// La explosión en curso; mientras haya una, la nave no existe y no hay controles.
    explosion: Option<Explosion>,
    /// Pedazos de la nave, bola de fuego y onda: se arman una vez al cargar.
    explosion_assets: ExplosionAssets,
    /// Tour automático (tecla T): mientras exista, él decide las teclas de vuelo.
    tour: Option<tour::Tour>,
    /// Apokolips, el mundo de Darkseid: aparece al juntar los 10 cristales (o con K).
    apokolips: apokolips::Apokolips,
    /// Pantalla de inicio: la cámara gira alrededor del sistema hasta que se aprieta Enter.
    title: bool,
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
        let explosion_assets = ExplosionAssets::new(&ship_mesh);

        let ship = Ship::new(Vec3::new(0.0, 3.0, 70.0));
        let camera = ChaseCamera::new(&ship);
        let uniforms = Game::make_uniforms(&camera, 0.0, true, 0.0, ship.position);
        // los cristales se reparten alrededor de los mundos, por eso el estado necesita la escena
        let scene = Scene::new();
        let state = GameState::new(&scene);
        let creatures = create_creatures(&scene);
        let oa = oa::Oa::new(&scene);
        let mundos = mundos::Mundos::new(&scene);
        let sun = scene.bodies.iter().position(|b| b.orbit_radius == 0.0 && b.parent.is_none());
        Game {
            renderer: Renderer::new(RENDER_W, RENDER_H, uniforms),
            scene,
            asteroids: AsteroidBelt::new(ASTEROID_SEED),
            comet: Comet::new(),
            creatures,
            oa,
            mundos,
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
            sun,
            heat: 0.0,
            explosion: None,
            explosion_assets,
            tour: None,
            apokolips: apokolips::Apokolips::new(),
            title: false,
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
            eclipse: (Vec3::ZERO, 0.0),
        }
    }

    /// Un cuadro del juego. Devuelve las teclas que de verdad se usaron (las del jugador o,
    /// en el tour, las del piloto automático), para que el sonido reaccione a las mismas.
    fn update(&mut self, input: ShipInput, dt: f32) -> ShipInput {
        // pantalla de inicio: solo corre el tiempo y la cámara gira alrededor del sistema
        if self.title {
            self.time += dt;
            self.title_camera();
            return ShipInput::default();
        }

        // Apokolips sigue apareciendo aunque la nave esté explotando (en modo foto se congela)
        let world_dt = if self.state.photo_mode { 0.0 } else { dt };
        self.apokolips.update(&mut self.scene, world_dt);

        // el tour decide las teclas, salvo que el jugador apriete alguna de vuelo
        let input = self.drive_tour(input, dt);

        // paso 0: si la nave está explotando no hay vuelo, va por otro camino
        if self.explosion.is_some() {
            self.update_exploding(input, dt);
            return input;
        }

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

        // paso 3a: el premio de los cristales: con los 10 se abre el Boom Tube de Apokolips
        if self.state.total > 0 && self.state.collected >= self.state.total {
            self.unlock_apokolips();
        }

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

        // paso 5: el calor del Sol, y si la nave lo tocó, explota.
        // La colisión la saca justo a su radio, así que "tocarlo" es estar a ~0 de él.
        if let Some(s) = self.sun {
            self.heat = explosion::heat_near(&self.scene, s, self.time, self.ship.position, HEAT_RANGE);
            let gap = (self.scene.body_position(s, self.time) - self.ship.position).length()
                - ship::collision_radius(self.scene.bodies[s].radius);
            if gap < 0.05 && !self.state.photo_mode {
                self.explode();
            }
        }
        input
    }

    /// Si hay tour, sus teclas reemplazan a las del jugador. Cualquier tecla de vuelo del
    /// jugador (o una explosión) lo termina. En modo foto el tour se pausa.
    fn drive_tour(&mut self, input: ShipInput, dt: f32) -> ShipInput {
        if self.tour.is_some() && (tour::player_took_control(&input) || self.explosion.is_some()) {
            self.tour = None;
        }
        match &mut self.tour {
            Some(t) if !self.state.photo_mode => {
                let mut auto = t.drive(&self.ship, &self.state, &self.scene, self.time, dt);
                auto.photo = input.photo; // la P sigue siendo del jugador
                auto
            }
            _ => input,
        }
    }

    /// Abre el Boom Tube y aparece Apokolips (premio de los cristales, o tecla K en la demo).
    /// Si ya había aparecido no hace nada.
    fn unlock_apokolips(&mut self) {
        self.apokolips.unlock(&mut self.scene, self.ship.position, self.time);
    }

    /// Tecla T: empieza el tour (desde el destino elegido) o lo termina.
    fn toggle_tour(&mut self) {
        self.tour = match self.tour {
            Some(_) => None,
            None if self.explosion.is_none() => Some(tour::Tour::new(&self.scene, self.state.warp_target)),
            None => None,
        };
    }

    /// Cámara de la pantalla de inicio: da vueltas despacio alrededor del Sol, desde arriba.
    fn title_camera(&mut self) {
        let a = self.time * TITLE_SPIN;
        self.camera.position = Vec3::new(a.cos() * TITLE_DISTANCE, TITLE_HEIGHT, a.sin() * TITLE_DISTANCE);
        self.camera.target = Vec3::ZERO;
        self.camera.up = Vec3::UP;
    }

    /// Enter en la pantalla de inicio: la cámara vuelve detrás de la nave y empieza el juego.
    fn start(&mut self) {
        self.title = false;
        self.camera = ChaseCamera::new(&self.ship);
    }

    /// La nave tocó el Sol: se cancela todo lo que estaba haciendo y empieza la explosión.
    fn explode(&mut self) {
        self.ship.cancel_warp();
        self.ship.release_orbit();
        self.ship.velocity = Vec3::ZERO;
        self.ship.boost = 0.0;
        self.state.scanned = None;
        let center = self.sun.map_or(Vec3::ZERO, |s| self.scene.body_position(s, self.time));
        self.explosion = Some(Explosion::new(self.ship.model_matrix(1.0), self.ship.position - center));
        self.heat = 0.0;
        self.warp = 0.0;
    }

    /// Un cuadro de la explosión. Solo funcionan P (foto) y 1-5 (elegir dónde reaparecer).
    fn update_exploding(&mut self, input: ShipInput, dt: f32) {
        if !self.state.photo_mode {
            self.time += dt;
            if let Some(e) = &mut self.explosion {
                e.age += dt;
            }
        }
        let allowed = ShipInput { select: input.select, photo: input.photo, ..Default::default() };
        interaction::update(&mut self.state, &mut self.ship, &self.scene, &allowed, self.time, dt);
        // la cámara se queda donde estaba, mirando la explosión
        if let Some(e) = &self.explosion {
            self.camera.target = e.center;
            if e.age >= EXPLOSION_TIME {
                self.respawn();
            }
        }
    }

    /// Termina la explosión: nave nueva junto al mundo elegido como destino (teclas 1-5).
    fn respawn(&mut self) {
        let gravity_on = self.ship.gravity_on;
        let (pos, center, velocity) = explosion::respawn_point(&self.scene, self.state.warp_target, self.time);
        self.ship = Ship::new(pos);
        self.ship.gravity_on = gravity_on;
        aim_ship_at(self, pos, center);
        self.ship.velocity = velocity;
        self.explosion = None;
    }

    /// Cuánto se mueve la cámara este cuadro: fuerte al explotar y un temblor con el calor.
    fn shake(&self) -> Vec3 {
        let (amount, t) = match &self.explosion {
            Some(e) => (SHAKE_STRENGTH * (1.0 - e.age / SHAKE_TIME).max(0.0).powi(2), e.age),
            None => (HEAT_SHAKE * self.heat * self.heat, self.time),
        };
        // tres senos de frecuencias distintas: parece al azar pero depende solo del tiempo
        Vec3::new((t * 53.0).sin(), (t * 71.0 + 1.3).sin(), (t * 61.0 + 2.1).sin()) * amount
    }

    fn render(&mut self) {
        self.draw_geometry();
        self.shade();
        self.draw_overlay();
    }

    /// HUD dibujado en NUESTRO framebuffer: el radar y el fondo de la ficha.
    /// Va después del sombreado porque se pinta encima de la imagen final.
    fn draw_overlay(&mut self) {
        if self.title {
            // en la pantalla de inicio no hay radar: solo se oscurece el centro para el título
            let mut cv = Canvas { px: &mut self.renderer.color, w: RENDER_W, h: RENDER_H };
            hud::draw_vignette(&mut cv, Vec3::ZERO, TITLE_SHADE, TITLE_SHADE * 0.2);
            return;
        }
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
        // Aviso de calor: la pantalla se pone roja, más en las orillas, y late cuando es peligroso.
        if self.heat > 0.01 {
            let pulse = if self.heat > HEAT_DANGER { 0.75 + 0.25 * (self.time * 8.0).sin() } else { 1.0 };
            hud::draw_vignette(&mut cv, HEAT_COLOR, self.heat * 0.12, self.heat * 0.65 * pulse);
        }
        // Destello blanco al explotar.
        if let Some(e) = &self.explosion {
            let flash = (1.0 - e.age / FLASH_TIME).max(0.0).powi(2);
            if flash > 0.0 {
                hud::draw_vignette(&mut cv, FLASH_COLOR, flash, flash);
            }
        }
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
        // la sacudida solo mueve lo que se dibuja, no la cámara de verdad (así no se acumula)
        let mut camera = self.camera.clone();
        let shake = self.shake();
        camera.position += shake;
        camera.target += shake;
        let mut u = Game::make_uniforms(&camera, self.time, self.normal_mapping, self.warp, self.ship.position);
        // La luna Central City puede tapar el sol sobre Gotham (eclipse).
        u.eclipse = mundos::gotham::eclipse(&self.scene, self.time);
        self.renderer.begin_frame(u);
        self.scene.draw(&mut self.renderer, self.time);
        self.state.draw_collectibles(&mut self.renderer, &self.scene, self.time);
        self.asteroids.draw(&mut self.renderer, self.time);
        self.comet.draw_nucleus(&mut self.renderer, self.time);
        creatures::draw_creatures(&mut self.renderer, &self.scene, self.time, &self.creatures);
        if let Some(oa) = &self.oa {
            oa.draw(&mut self.renderer, &self.scene, self.time);
        }
        self.mundos.draw(&mut self.renderer, &self.scene, self.time);
        self.apokolips.draw(&mut self.renderer, &self.scene, self.time);
        let model = self.ship.model_matrix(1.0);
        if let Some(e) = &self.explosion {
            // la nave ya no está: en su lugar van los pedazos
            e.draw(&mut self.renderer, &self.explosion_assets);
        } else {
            // El shader de la nave recibe el estado del motor y el calor en cada cuadro (sus "uniforms" propios).
            let ship_shader: Arc<dyn FragmentShader> = if self.ship_is_halcon {
                Arc::new(HalconShader { engine_glow: self.ship.engine_glow(), heat: self.heat })
            } else {
                Arc::new(ShipShader { base: SHIP_COLOR, boost: self.ship.boost })
            };
            self.renderer.draw_mesh(&self.ship_mesh, &model, ship_shader);
        }

        // Lo aditivo (la cola y el halo del cometa, la llama del turbo) va SIEMPRE al final,
        // porque necesita el z-buffer completo de lo opaco.
        self.state.draw_race(&mut self.renderer, &self.scene, self.time);
        self.comet.draw_glow(&mut self.renderer, self.time);
        creatures::draw_creature_glows(&mut self.renderer, &self.scene, self.time, &self.creatures);
        if let Some(oa) = &self.oa {
            oa.draw_glow(&mut self.renderer, &self.scene, self.time);
        }
        self.mundos.draw_glow(&mut self.renderer, &self.scene, self.time);
        self.apokolips.draw_glow(&mut self.renderer, &self.scene, self.time);
        if let Some(e) = &self.explosion {
            e.draw_glow(&mut self.renderer, &self.explosion_assets);
        }

        // Llama del turbo: SIEMPRE al final, porque es aditiva y necesita el z-buffer completo.
        if self.ship.boost > 0.01 && self.explosion.is_none() {
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
        let eye = hero + (out * 0.6 + side * 0.8 + Vec3::UP * 0.3) * (size * PORTRAIT_DISTANCE);
        g.ship.position = eye + (eye - hero).normalized() * 30.0;
        g.camera.position = eye;
        g.camera.target = hero;
        g.camera.up = Vec3::UP;
        g.render();
        let file = format!("snap_{}.ppm", name.to_lowercase().replace(' ', "_"));
        g.renderer.save_ppm(&file).expect("no se pudo guardar");
        println!("{file}: listo");
    }

    // Batiseñal: vista de lado del haz que sale de la azotea de Batman, con Gotham abajo.
    if let Some(b) = g.creatures.iter().find(|c| c.name == "Batman") {
        let model = b.model(&g.scene, g.time);
        let (hero, up) = (b.position(&g.scene, g.time), model.transform_dir(Vec3::UP).normalized());
        let across = up.cross(model.transform_dir(Vec3::new(1.0, 0.0, 0.0))).normalized();
        let target = hero + up * 4.0;
        g.camera.position = target + across * 11.0 + up * 1.5;
        g.camera.target = target;
        g.camera.up = up;
        g.ship.position = g.camera.position + up * 40.0; // nave fuera de cuadro
        g.render();
        g.renderer.save_ppm("snap_batisenal.ppm").expect("no se pudo guardar");
        println!("snap_batisenal.ppm: listo");
    }

    // Oa: la Batería Central de cerca (con torres y Linterna Verde) y el planeta con su anillo.
    g.time = 2.0; // en t = 2 s el puño (constructo) de Oa ya apareció
    if let Some((base, up)) = g.oa.as_ref().map(|oa| oa.battery_spot(&g.scene, g.time)) {
        let across = up.cross(Vec3::UP).normalized();
        let target = base + up * 1.2;
        g.camera.position = target + across * 4.5 + up * 0.8;
        g.camera.target = target;
        g.camera.up = up;
        g.ship.position = g.camera.position + up * 40.0;
        g.render();
        g.renderer.save_ppm("snap_oa_bateria.ppm").expect("no se pudo guardar");
        let oa_i = g.scene.bodies.iter().position(|b| b.name == "Oa").unwrap_or(0);
        let center = g.scene.body_position(oa_i, g.time);
        let to_sun = (-center).normalized();
        let oa_radius = g.scene.bodies[oa_i].radius;
        aim_ship_at(&mut g, center + (to_sun * 5.5 + Vec3::UP * 2.5) * oa_radius, center);
        g.render();
        g.renderer.save_ppm("snap_oa_anillo.ppm").expect("no se pudo guardar");
        println!("snap_oa_*.ppm: listo");
    }
    g.time = 0.0;

    // Eclipse: busca el momento en que la luna Central City pasa entre el sol y Gotham y mira
    // a Gotham desde el lado del sol, para ver la sombra.
    if let (Some(gi), Some(ci)) = (
        g.scene.bodies.iter().position(|b| b.name == "Gotham"),
        g.scene.bodies.iter().position(|b| b.name == "Central City"),
    ) {
        let lined_up = |t: f32| {
            let (gp, cp) = (g.scene.body_position(gi, t), g.scene.body_position(ci, t));
            (cp - gp).normalized().dot((-gp).normalized()) // 1 = la luna justo hacia el sol
        };
        let best = (0..400).map(|k| k as f32 * 0.05).fold(0.0f32, |b, t| if lined_up(t) > lined_up(b) { t } else { b });
        let saved = g.time;
        g.time = best;
        let center = g.scene.body_position(gi, best);
        let to_sun = (-center).normalized();
        let gotham_radius = g.scene.bodies[gi].radius;
        aim_ship_at(&mut g, center + (to_sun * 3.2 + Vec3::UP * 1.2) * gotham_radius, center);
        g.render();
        g.renderer.save_ppm("snap_eclipse.ppm").expect("no se pudo guardar");
        g.time = saved;
    }

    // Monumentos de cada mundo (carpeta mundos/): vista de lado, un poco desde arriba.
    for (name, base, up, size) in g.mundos.landmarks(&g.scene, g.time) {
        let across = up.cross(if up.y.abs() < 0.9 { Vec3::UP } else { Vec3::new(1.0, 0.0, 0.0) }).normalized();
        let target = base + up * (size * 0.5);
        g.camera.position = target + across * (size * 3.0) + up * (size * 1.2);
        g.camera.target = target;
        g.camera.up = up;
        g.ship.position = g.camera.position + up * 40.0;
        g.render();
        g.renderer.save_ppm(&format!("snap_monumento_{name}.ppm")).expect("no se pudo guardar");
    }

    // Un retrato de cada mundo desde su lado iluminado, con la nave en primer plano,
    // y otro a ras de suelo junto a uno de sus objetos de superficie (árboles, cristales...).
    for i in 0..g.scene.bodies.len() {
        let (name, radius) = (g.scene.bodies[i].name, g.scene.bodies[i].radius);
        if g.scene.bodies[i].orbit_radius == 0.0 {
            continue; // el sol
        }
        let planet = g.scene.body_position(i, g.time);
        let to_sun = (-planet).normalized();
        let side = to_sun.cross(Vec3::UP).normalized();
        let file = name.to_lowercase().replace(' ', "_");
        aim_ship_at(&mut g, planet + (to_sun * 2.4 + side * 1.0 + Vec3::UP * 0.5) * radius, planet);
        g.render();
        g.renderer.save_ppm(&format!("snap_mundo_{file}.ppm")).expect("no se pudo guardar");

        // El objeto de superficie que más mira al sol (para que salga iluminado).
        let frame = g.scene.body_frame(i, g.time);
        let b = &g.scene.bodies[i];
        let best = b.props.iter().max_by(|p, q| {
            let lit = |d: Vec3| frame.transform_dir(d).dot(to_sun);
            lit(p.dir).total_cmp(&lit(q.dir))
        });
        if let Some(p) = best {
            let ground = frame.transform_point(p.dir * (radius * (b.surface)(p.dir)));
            let up = frame.transform_dir(p.dir).normalized();
            let across = up.cross(Vec3::UP).normalized();
            // Cámara un poco arriba y a un lado del objeto, mirando hacia él y al horizonte.
            let eye = ground + up * (SURFACE_SHOT_HEIGHT * radius) + across * (SURFACE_SHOT_DISTANCE * radius);
            g.ship.position = eye + up * 40.0; // la nave fuera de cuadro
            g.camera.position = eye;
            g.camera.target = ground + up * (0.05 * radius);
            g.camera.up = up;
            g.render();
            g.renderer.save_ppm(&format!("snap_suelo_{file}.ppm")).expect("no se pudo guardar");
        }
        println!("snap_mundo_{file}.ppm: listo");
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

    // Calor y explosión: la nave a 4 unidades de la colisión del Sol, volando de lado.
    if let Some(s) = g.sun {
        let sun = g.scene.body_position(s, g.time);
        let reach = ship::collision_radius(g.scene.bodies[s].radius);
        let pos = sun + Vec3::new(0.0, 0.0, reach + 4.0);
        aim_ship_at(&mut g, pos, pos + Vec3::new(30.0, 0.0, 0.0));
        g.heat = explosion::heat_near(&g.scene, s, g.time, pos, HEAT_RANGE);
        g.render();
        g.renderer.save_ppm("snap_calor.ppm").expect("no se pudo guardar");
        // La misma nave explota; se fotografía de lado a los 0.35 s (fuego) y a 1.2 s (pedazos).
        g.explode();
        for (age, file) in [(0.35, "snap_explosion.ppm"), (1.2, "snap_explosion_pedazos.ppm")] {
            let (center, away) = match &mut g.explosion {
                Some(e) => {
                    e.age = age;
                    (e.center, e.away)
                }
                None => break,
            };
            g.camera.position = center + away * 10.0 + Vec3::UP * 4.0 + Vec3::new(8.0, 0.0, 0.0);
            g.camera.target = center;
            g.camera.up = Vec3::UP;
            g.render();
            g.renderer.save_ppm(file).expect("no se pudo guardar");
        }
        g.explosion = None;
        g.heat = 0.0;
        println!("snap_calor.ppm y snap_explosion*.ppm: listo");
    }

    // Apokolips: el Boom Tube a medio abrir, el planeta, Darkseid y su nave.
    let ship_before = (g.ship.position, g.time);
    g.time = 0.0;
    g.ship.position = Vec3::new(0.0, 25.0, 150.0);
    g.unlock_apokolips();
    if let Some(i) = g.apokolips.index() {
        let world = g.scene.body_position(i, g.time);
        // el portal a los 2.5 s: ya está abierto y el planeta va creciendo
        g.apokolips.update(&mut g.scene, 2.5);
        aim_ship_at(&mut g, world + Vec3::new(0.0, 8.0, 40.0), world);
        g.render();
        g.renderer.save_ppm("snap_boom_tube.ppm").expect("no se pudo guardar");
        g.apokolips.finish(&mut g.scene);
        let to_sun = (-world).normalized();
        let r = g.scene.bodies[i].radius;
        aim_ship_at(&mut g, world + (to_sun * 2.6 + Vec3::UP * 0.8) * r, world);
        g.render();
        g.renderer.save_ppm("snap_mundo_apokolips.ppm").expect("no se pudo guardar");
        if let Some((feet, up)) = g.apokolips.darkseid_spot(&g.scene, g.time) {
            let across = up.cross(if up.y.abs() < 0.9 { Vec3::UP } else { Vec3::new(1.0, 0.0, 0.0) }).normalized();
            let target = feet + up * 1.4;
            g.camera.position = target + across * 4.5 + up * 1.2;
            g.camera.target = target;
            g.camera.up = up;
            g.ship.position = g.camera.position + up * 40.0;
            g.render();
            g.renderer.save_ppm("snap_darkseid.ppm").expect("no se pudo guardar");
        }
        if let Some(m) = g.apokolips.ship_model(&g.scene, g.time) {
            let pos = m.transform_point(Vec3::ZERO);
            g.camera.position = pos + Vec3::new(4.0, 2.0, 5.0);
            g.camera.target = pos;
            g.camera.up = Vec3::UP;
            g.ship.position = g.camera.position + Vec3::UP * 40.0;
            g.render();
            g.renderer.save_ppm("snap_nave_apokolips.ppm").expect("no se pudo guardar");
        }
        println!("snap_boom_tube, mundo_apokolips, darkseid y nave_apokolips: listo");
    }
    (g.ship.position, g.time) = ship_before;

    // Pantalla de inicio (sin los textos, que los escribe raylib en la ventana).
    g.title = true;
    g.update(ShipInput::default(), 0.0);
    g.render();
    g.renderer.save_ppm("snap_titulo.ppm").expect("no se pudo guardar");
    g.title = false;
    println!("snap_titulo.ppm: listo");

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

/// Qué tan lejos se pone la cámara en los retratos de los personajes (en múltiplos de su tamaño).
const PORTRAIT_DISTANCE: f32 = 3.5;

/// Toma a ras de suelo: altura y distancia de la cámara al objeto (en múltiplos del radio del mundo).
const SURFACE_SHOT_HEIGHT: f32 = 0.12;
const SURFACE_SHOT_DISTANCE: f32 = 0.35;

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

/// Escribe un texto centrado a lo ancho del lienzo del HUD, a la altura `y`.
macro_rules! draw_centered {
    ($d:expr, $text:expr, $y:expr, $size:expr, $color:expr) => {{
        let text: &str = $text;
        let w = $d.measure_text(text, $size);
        $d.draw_text(text, (WINDOW_W - w) / 2, $y, $size, $color);
    }};
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
        race_toggle: rl.is_key_pressed(KeyboardKey::KEY_R),
    }
}

// revisa las teclas 1 a 6 y devuelve el numero de la q se acaba de apretar
fn read_world_key(rl: &RaylibHandle) -> Option<usize> {
    let keys = [
        KeyboardKey::KEY_ONE,
        KeyboardKey::KEY_TWO,
        KeyboardKey::KEY_THREE,
        KeyboardKey::KEY_FOUR,
        KeyboardKey::KEY_FIVE,
        KeyboardKey::KEY_SIX,
    ];
    // la tecla 1 esta en la posicion 0 de la lista, por eso el + 1
    keys.iter().position(|&key| rl.is_key_pressed(key)).map(|i| i + 1)
}

fn main() {
    if std::env::args().any(|a| a == "--snapshot") {
        snapshot();
        return;
    }

    let (mut rl, thread) =
        raylib::init().size(WINDOW_W, WINDOW_H).resizable().title("Sistema Solar - Software Renderer").build();
    rl.set_target_fps(60);

    // PANTALLA COMPLETA: todo (juego + HUD) se dibuja en un lienzo fijo de WINDOW_W×WINDOW_H y al
    // final ese lienzo se escala para llenar la pantalla sin deformarse (franjas negras si el
    // monitor no es 16:9). Así el HUD queda igual en ventana y en pantalla completa.
    let mut canvas = rl
        .load_render_texture(&thread, WINDOW_W as u32, WINDOW_H as u32)
        .expect("no se pudo crear el lienzo");
    if START_FULLSCREEN {
        rl.toggle_borderless_windowed();
    }

    let mut game = Game::new();
    game.title = true; // se empieza en la pantalla de inicio (el snapshot y los tests no)

    // SONIDO: si no hay tarjeta de sonido el juego sigue, solo que callado.
    // `sonidos` se declara después de `audio` para que se libere antes (sus sonidos dependen de él).
    let audio = RaylibAudio::init_audio_device().ok().filter(|a| a.is_audio_device_ready());
    if audio.is_none() {
        eprintln!("No se pudo abrir el audio: se juega sin sonido");
    }
    let mut sonidos = audio.as_ref().map(|a| audio::Sonidos::new(a, &game.scene));

    // Textura donde se copia nuestro framebuffer cada cuadro.
    let image = Image::gen_image_color(RENDER_W as i32, RENDER_H as i32, Color::BLACK);
    let mut texture = rl.load_texture_from_image(&thread, &image).expect("no se pudo crear la textura");
    // empieza oculta: solo un avisito con la tecla, para no tapar la pantalla todo el rato
    let mut show_help = false;

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
        if rl.is_key_pressed(KeyboardKey::KEY_F11) {
            rl.toggle_borderless_windowed();
        }
        // todas las teclas de prender/apagar de arriba suenan con un clic
        let toggles = [KeyboardKey::KEY_C, KeyboardKey::KEY_L, KeyboardKey::KEY_O, KeyboardKey::KEY_N, KeyboardKey::KEY_H, KeyboardKey::KEY_F11];
        if let Some(s) = &mut sonidos {
            if toggles.iter().any(|&k| rl.is_key_pressed(k)) {
                s.tocar(audio::Efecto::Interruptor);
            }
            if rl.is_key_pressed(KeyboardKey::KEY_M) {
                s.tocar(audio::Efecto::Interruptor);
                s.alternar_silencio();
            }
        }

        // Pantalla de inicio: Enter empieza a jugar y T empieza directo con el tour automático.
        // Ya jugando, T prende o apaga el tour.
        let tour_key = rl.is_key_pressed(KeyboardKey::KEY_T);
        if game.title {
            if rl.is_key_pressed(KeyboardKey::KEY_ENTER) || tour_key {
                game.start();
                if tour_key {
                    game.toggle_tour();
                }
                if let Some(s) = &sonidos {
                    s.tocar(audio::Efecto::Destino);
                }
            }
        } else if rl.is_key_pressed(KeyboardKey::KEY_K) {
            game.unlock_apokolips(); // atajo para la demo: abre el Boom Tube sin juntar los cristales
        }
        if !game.title && tour_key {
            game.toggle_tour();
            if let Some(s) = &sonidos {
                s.tocar(audio::Efecto::Interruptor);
            }
        }

        let input = read_input(&rl);
        let used = game.update(input, dt);
        if let Some(s) = &mut sonidos {
            s.update(&game, &used, dt);
        }
        game.render();
        texture.update_texture(&game.renderer.color).expect("no se pudo actualizar la textura");

        let fps = rl.get_fps();
        let (screen_w, screen_h) = (rl.get_screen_width() as f32, rl.get_screen_height() as f32);
        let mut frame = rl.begin_drawing(&thread);
        {
            // Todo lo de este bloque se dibuja en el lienzo de WINDOW_W×WINDOW_H, no en la pantalla.
            let mut d = frame.begin_texture_mode(&thread, &mut canvas);
            d.clear_background(Color::BLACK);
            d.draw_texture_pro(
                &texture,
                Rectangle::new(0.0, 0.0, RENDER_W as f32, RENDER_H as f32),
                Rectangle::new(0.0, 0.0, WINDOW_W as f32, WINDOW_H as f32),
                Vector2::zero(),
                0.0,
                Color::WHITE,
            );

            if game.title {
                // PANTALLA DE INICIO (el sistema gira de fondo; el centro ya viene oscurecido)
                let y = WINDOW_H / 2;
                draw_centered!(d, "SISTEMA SOLAR", y - 210, 72, Color::RAYWHITE);
                draw_centered!(d, "un recorrido por los mundos de la Liga", y - 128, 26, Color::GOLD);
                draw_centered!(d, "Renderer por software  -  Graficas por Computadora 2026", y - 92, 20, Color::LIGHTGRAY);
                draw_centered!(d, "W/S acelerar   A/D girar   Flechas subir/bajar   Shift turbo   Espacio frenar", y - 20, 18, Color::SKYBLUE);
                draw_centered!(d, "F orbita   R minijuego   E escanear   1-5 destino   Tab warp   P foto   M sonido   H ayuda   K portal", y + 6, 18, Color::SKYBLUE);
                if (game.time * 3.0).sin() > -0.4 {
                    draw_centered!(d, "ENTER para empezar", y + 90, 34, Color::RAYWHITE);
                }
                draw_centered!(d, "T  para ver el tour automatico", y + 134, 22, Color::GOLD);
            } else {
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
                let sound = match &sonidos {
                    Some(s) if s.silencio => "OFF",
                    Some(_) => "ON",
                    None => "sin audio",
                };
                d.draw_text(
                    &format!(
                        "culling [C]: {}   LOD [L]: {}   orbitas [O]: {}   normales [N]: {}   gravedad [G]: {}   sonido [M]: {sound}",
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
                let worlds = game.scene.worlds().len();
                let crystals = if game.state.total > 0 {
                    format!("{}/{}", game.state.collected, game.state.total)
                } else {
                    "-".to_string()
                };
                d.draw_text(
                    &format!(
                        "destino [1-{worlds}]: {target}   warp [Tab]: {}   cristales: {crystals}",
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

                // Minijuego del mundo: distinto segun el mundo (circuito de aros, esquiva de
                // peligros o tocar objetivos). Progreso si esta en curso, si no un aviso para
                // empezarlo (solo se puede en orbita) y, al terminar, el resultado.
                let minigame_color = Color::new(255, 90, 165, 255);
                if let Some((body, hud)) = game.state.minigame_hud() {
                    let world = hud::to_ascii(game.scene.bodies[body].name);
                    let text = match hud {
                        interaction::MinigameHud::Gates { next, total, elapsed, gate_left } => match gate_left {
                            Some(left) => format!("{world}: aro {}/{total}   tiempo: {elapsed:.1}s   se apaga en {left:.1}s   [R] cancelar", next + 1),
                            None => format!("{world}: aro {}/{total}   tiempo: {elapsed:.1}s   [R] cancelar", next + 1),
                        },
                        interaction::MinigameHud::Survive { elapsed, goal } => {
                            format!("{world}: sobrevive {elapsed:.1}/{goal:.0}s esquivando los cristales   [R] cancelar")
                        }
                        interaction::MinigameHud::Collect { touched, total, remaining } => {
                            format!("{world}: objetivos {touched}/{total}   tiempo restante: {remaining:.1}s   [R] cancelar")
                        }
                    };
                    d.draw_text(&text, 10, 124, 20, minigame_color);
                } else if let Some(i) = game.state.orbit_lock {
                    if let Some(name) = game.state.minigame_name(i) {
                        d.draw_text(&format!("[R] iniciar: {name}"), 10, 124, 20, minigame_color);
                    }
                }
                if let Some(result) = &game.state.race_result {
                    let world = hud::to_ascii(game.scene.bodies[result.body].name);
                    let text = format!("{world}: {}", hud::to_ascii(&result.text));
                    let color = if result.success { Color::new(110, 255, 150, 255) } else { Color::new(255, 110, 90, 255) };
                    draw_centered!(d, &text, 94, 24, color);
                }

                // Letreros al centro: la explosión o, si no, el aviso de calor.
                if let Some(e) = &game.explosion {
                    let left = (EXPLOSION_TIME - e.age).max(0.0).ceil() as i32;
                    let world = hud::to_ascii(game.scene.bodies[game.state.warp_target].name);
                    draw_centered!(d, "NAVE DESTRUIDA", WINDOW_H / 2 - 80, 48, Color::new(255, 90, 40, 255));
                    draw_centered!(d, &format!("reapareciendo en {left} s junto a {world}"), WINDOW_H / 2 - 22, 22, Color::RAYWHITE);
                    draw_centered!(d, &format!("[1-{}] elegir otro mundo", game.scene.worlds().len()), WINDOW_H / 2 + 8, 18, Color::GOLD);
                } else if game.heat > HEAT_WARNING {
                    // cuando ya es peligroso el letrero parpadea
                    if game.heat < HEAT_DANGER || (game.time * 6.0).sin() > -0.3 {
                        draw_centered!(d, "ALERTA: CALOR EXTREMO", 130, 30, Color::new(255, 80, 30, 255));
                        draw_centered!(d, "alejate del Sol o la nave explota", 166, 20, Color::ORANGE);
                    }
                }

                // Letrero del premio: se abrió el Boom Tube de Apokolips.
                if let Some(age) = game.apokolips.age().filter(|&a| a < apokolips::MESSAGE_TIME) {
                    if game.explosion.is_none() {
                        let key = game.scene.worlds().len();
                        draw_centered!(d, "SE ABRIO UN BOOM TUBE", 200, 34, Color::new(120, 190, 255, 255));
                        if age > 1.0 {
                            draw_centered!(d, &format!("Apokolips, el mundo de Darkseid, aparecio: tecla {key} para viajar"), 242, 20, Color::RAYWHITE);
                        }
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

                // Tour automático: qué está haciendo y cómo salir.
                if let Some(t) = &game.tour {
                    let text = format!("TOUR AUTOMATICO: {}", hud::to_ascii(&t.describe(&game.scene)));
                    draw_centered!(d, &text, WINDOW_H - 140, 24, Color::GOLD);
                    draw_centered!(d, "cualquier tecla de vuelo o T para tomar el control", WINDOW_H - 110, 18, Color::LIGHTGRAY);
                }

                if show_help {
                    // son tres lineas porque en menos ya no cabia en la ventana
                    let help = [
                        "W/S acelerar/frenar   A/D girar   Flechas subir/bajar   Shift turbo   Espacio frenar   F orbita   E escanear".to_string(),
                        format!("1-{} destino   Tab warp/cancelar   G gravedad   P foto   T tour   K portal (demo)   R minijuego (en orbita)", game.scene.worlds().len()),
                        "C culling   L LOD   O orbitas   N normales   M sonido   H ocultar   F11 pantalla".to_string(),
                    ];
                    for (k, line) in help.iter().enumerate() {
                        d.draw_text(line, 10, WINDOW_H - 70 + 22 * k as i32, 16, Color::GOLD);
                    }
                } else {
                    // oculta: dejamos solo el avisito pa que no se les olvide la tecla
                    d.draw_text("[H] controles", 10, WINDOW_H - 26, 16, Color::GRAY);
                }
            }
        }

        // El lienzo se escala lo más grande que quepa en la pantalla, centrado y sin deformarse.
        let scale = (screen_w / WINDOW_W as f32).min(screen_h / WINDOW_H as f32);
        let (w, h) = (WINDOW_W as f32 * scale, WINDOW_H as f32 * scale);
        frame.clear_background(Color::BLACK);
        frame.draw_texture_pro(
            &canvas,
            // alto negativo: las texturas de dibujo de OpenGL quedan de cabeza, así se voltean
            Rectangle::new(0.0, 0.0, WINDOW_W as f32, -(WINDOW_H as f32)),
            Rectangle::new((screen_w - w) * 0.5, (screen_h - h) * 0.5, w, h),
            Vector2::zero(),
            0.0,
            Color::WHITE,
        );
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

    // pone la nave casi tocando el cuerpo i, bajando hacia él a 20
    fn ship_falling_on(g: &mut Game, i: usize) {
        let center = g.scene.body_position(i, g.time);
        let reach = ship::collision_radius(g.scene.bodies[i].radius);
        g.ship.position = center + Vec3::new(0.0, reach + 0.2, 0.0);
        g.ship.velocity = Vec3::new(0.0, -20.0, 0.0);
    }

    // corre cuadros vacios hasta q termine la explosion (con tope por si nunca acaba)
    fn wait_explosion(g: &mut Game) {
        let mut t = 0.0;
        while g.explosion.is_some() && t < EXPLOSION_TIME + 1.0 {
            g.update(ShipInput::default(), 1.0 / 60.0);
            t += 1.0 / 60.0;
        }
    }

    #[test]
    fn el_sol_destruye_la_nave_y_reaparece_en_el_destino() {
        let mut g = Game::new();
        let dt = 1.0 / 60.0;
        g.state.warp_target = 3;
        let sun = g.sun.unwrap();
        ship_falling_on(&mut g, sun);
        g.update(ShipInput::default(), dt);
        assert!(g.explosion.is_some());

        // explotando no hay controles: ni acelerar, ni warp, ni orbita
        let pos = g.ship.position;
        g.update(ShipInput { thrust: 1.0, warp: true, orbit_toggle: true, ..Default::default() }, dt);
        assert_eq!((g.ship.position - pos).length(), 0.0);
        assert!(!g.ship.warping());

        // al terminar reaparece junto a Gotham, lista pa enganchar la orbita con F
        wait_explosion(&mut g);
        assert!(g.explosion.is_none());
        g.update(ShipInput::default(), dt);
        assert_eq!(g.state.nearest, Some(3));
        assert!(interaction::orbit_in_range(&g.state, &g.scene));
        assert_eq!(g.heat, 0.0);
    }

    #[test]
    fn los_planetas_no_destruyen_la_nave() {
        let mut g = Game::new();
        ship_falling_on(&mut g, 1);
        g.update(ShipInput::default(), 1.0 / 60.0);
        assert!(g.explosion.is_none());
        assert!(g.ship.impact > 0.0, "si choco, pero solo rebota");
    }

    #[test]
    fn el_calor_sube_al_acercarse_al_sol() {
        let mut g = Game::new();
        let dt = 1.0 / 60.0;
        // donde arranca el juego no hay calor
        g.update(ShipInput::default(), dt);
        assert_eq!(g.heat, 0.0);

        // a 4 de la colision ya es peligroso, pero todavia no explota
        let s = g.sun.unwrap();
        let reach = ship::collision_radius(g.scene.bodies[s].radius);
        g.ship.position = g.scene.body_position(s, g.time) + Vec3::new(0.0, 0.0, reach + 4.0);
        g.ship.velocity = Vec3::ZERO;
        g.update(ShipInput::default(), dt);
        assert!(g.heat > HEAT_DANGER, "calor {}", g.heat);
        assert!(g.explosion.is_none());
    }

    #[test]
    fn en_modo_foto_la_explosion_se_congela() {
        let mut g = Game::new();
        let dt = 1.0 / 60.0;
        let sun = g.sun.unwrap();
        ship_falling_on(&mut g, sun);
        g.update(ShipInput::default(), dt);
        g.update(ShipInput { photo: true, ..Default::default() }, dt);
        let age = g.explosion.as_ref().unwrap().age;
        for _ in 0..(EXPLOSION_TIME / dt) as usize + 30 {
            g.update(ShipInput::default(), dt);
        }
        // pasaron mas de 3 s y la explosion sigue igual
        assert_eq!(g.explosion.as_ref().unwrap().age, age);

        // al salir del modo foto sigue y termina
        g.update(ShipInput { photo: true, ..Default::default() }, dt);
        wait_explosion(&mut g);
        assert!(g.explosion.is_none());
    }

    #[test]
    fn durante_la_explosion_se_elige_donde_reaparecer() {
        let mut g = Game::new();
        let dt = 1.0 / 60.0;
        let sun = g.sun.unwrap();
        ship_falling_on(&mut g, sun);
        g.update(ShipInput::default(), dt);
        g.update(ShipInput { select: Some(5), ..Default::default() }, dt);
        wait_explosion(&mut g);
        g.update(ShipInput::default(), dt);
        assert_eq!(g.state.nearest, Some(5));
    }

    #[test]
    fn el_tour_viaja_rodea_un_mundo_y_se_aleja() {
        let mut g = Game::new();
        let dt = 1.0 / 30.0;
        g.toggle_tour();
        let (mut warped, mut orbited, mut scanned) = (false, None, false);
        let mut t = 0.0;
        // un minuto de juego alcanza pa visitar el primer mundo completo
        while t < 60.0 && g.tour.as_ref().is_some_and(|tour| tour.laps_done == 0 || g.state.orbit_lock.is_some()) {
            g.update(ShipInput::default(), dt);
            warped |= g.ship.warping();
            orbited = orbited.or(g.state.orbit_lock);
            scanned |= g.state.scanned.is_some();
            assert!(g.explosion.is_none(), "el tour no debe chocar con el Sol");
            t += dt;
        }
        let tour = g.tour.as_ref().expect("el tour sigue andando");
        println!("primer mundo en {t:.1} s");
        assert!(warped, "viajo con warp");
        assert_eq!(orbited, Some(1), "se engancho a la orbita del primer destino");
        assert!(scanned, "mostro la ficha del mundo");
        assert_eq!(tour.laps_done, 1, "le dio la vuelta completa");
        assert_eq!(g.state.orbit_lock, None, "y la solto para alejarse");
    }

    #[test]
    fn el_tour_recorre_los_cinco_mundos_sin_chocar() {
        let mut g = Game::new();
        let dt = 1.0 / 30.0;
        g.toggle_tour();
        let mut visited = Vec::new();
        let mut t = 0.0;
        while t < 240.0 && visited.len() < 5 {
            g.update(ShipInput::default(), dt);
            assert!(g.explosion.is_none(), "el tour exploto en t = {t:.1}");
            if let Some(i) = g.state.orbit_lock {
                if !visited.contains(&i) {
                    visited.push(i);
                }
            }
            t += dt;
        }
        println!("orden de visita: {visited:?} en {t:.0} s");
        assert_eq!(visited, vec![1, 2, 3, 4, 5]);
    }

    #[test]
    fn una_tecla_de_vuelo_detiene_el_tour() {
        let mut g = Game::new();
        g.toggle_tour();
        g.update(ShipInput::default(), 1.0 / 60.0);
        assert!(g.tour.is_some());
        // la P (foto) no lo detiene: solo lo pausa
        g.update(ShipInput { photo: true, ..Default::default() }, 1.0 / 60.0);
        assert!(g.tour.is_some());
        g.update(ShipInput { photo: true, ..Default::default() }, 1.0 / 60.0);
        // acelerar si
        g.update(ShipInput { thrust: 1.0, ..Default::default() }, 1.0 / 60.0);
        assert!(g.tour.is_none());
    }

    #[test]
    fn la_pantalla_de_inicio_no_mueve_la_nave() {
        let mut g = Game::new();
        g.title = true;
        let pos = g.ship.position;
        for _ in 0..60 {
            g.update(ShipInput { thrust: 1.0, warp: true, ..Default::default() }, 1.0 / 60.0);
        }
        assert_eq!((g.ship.position - pos).length(), 0.0);
        assert!(g.time > 0.9, "los planetas si siguen girando");
        // la camara da vueltas lejos, mirando al Sol
        assert!((g.camera.position.length() - (TITLE_DISTANCE.powi(2) + TITLE_HEIGHT.powi(2)).sqrt()).abs() < 1e-2);
        g.start();
        assert!(!g.title);
        assert!((g.camera.position - g.ship.position).length() < 10.0, "la camara vuelve detras de la nave");
    }

    #[test]
    fn con_los_diez_cristales_aparece_apokolips() {
        let mut g = Game::new();
        let dt = 1.0 / 60.0;
        let worlds = g.scene.worlds().len();
        g.update(ShipInput::default(), dt);
        assert!(g.apokolips.age().is_none(), "sin cristales no aparece");

        // junto los 10 (lo de recogerlos ya lo prueba interaction.rs)
        g.state.collected = g.state.total;
        g.update(ShipInput::default(), dt);
        let i = g.apokolips.index().expect("se abrio el Boom Tube");
        assert_eq!(g.scene.bodies[i].name, "Apokolips");
        assert_eq!(g.scene.worlds().len(), worlds + 1);

        // a los 5 s ya tiene su tamaño y la tecla 6 lo elige como destino
        for _ in 0..(apokolips::PORTAL_CLOSE / dt) as usize + 5 {
            g.update(ShipInput::default(), dt);
        }
        assert_eq!(g.scene.bodies[i].radius, 5.0);
        g.update(ShipInput { select: Some(6), ..Default::default() }, dt);
        assert_eq!(g.state.warp_target, i);
    }

    #[test]
    fn la_tecla_6_no_hace_nada_antes_del_portal_y_k_lo_abre_una_vez() {
        let mut g = Game::new();
        let dt = 1.0 / 60.0;
        let target = g.state.warp_target;
        g.update(ShipInput { select: Some(6), ..Default::default() }, dt);
        assert_eq!(g.state.warp_target, target, "la 6 todavia no es ningun mundo");

        let bodies = g.scene.bodies.len();
        g.unlock_apokolips();
        g.unlock_apokolips();
        assert_eq!(g.scene.bodies.len(), bodies + 1, "Apokolips aparece una sola vez");
    }

    #[test]
    fn el_tour_tambien_visita_apokolips() {
        let mut g = Game::new();
        let dt = 1.0 / 30.0;
        g.unlock_apokolips();
        g.apokolips.finish(&mut g.scene);
        let apokolips = g.apokolips.index().unwrap();
        g.state.warp_target = apokolips;
        g.toggle_tour();
        let mut t = 0.0;
        while t < 60.0 && g.state.orbit_lock != Some(apokolips) {
            g.update(ShipInput::default(), dt);
            assert!(g.explosion.is_none(), "el tour exploto en t = {t:.1}");
            t += dt;
        }
        assert_eq!(g.state.orbit_lock, Some(apokolips), "se engancho a la orbita de Apokolips");
    }

    #[test]
    fn en_modo_foto_el_portal_se_congela() {
        let mut g = Game::new();
        let dt = 1.0 / 60.0;
        g.unlock_apokolips();
        g.update(ShipInput::default(), dt);
        g.update(ShipInput { photo: true, ..Default::default() }, dt);
        let age = g.apokolips.age().unwrap();
        for _ in 0..60 {
            g.update(ShipInput::default(), dt);
        }
        assert_eq!(g.apokolips.age().unwrap(), age);
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
