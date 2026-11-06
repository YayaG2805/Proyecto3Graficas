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

// --- giro de la nave, tambien se pueden mover en vivo ---
// cuanto sube la velocidad de giro por segundo mientras tenes la tecla (rad/s²)
const TURN_ACCEL: f32 = 6.0;
// lo mas rapido q puede girar (rad/s)
const TURN_MAX: f32 = 1.6;
// q tan rapido se frena el giro al soltar la tecla, mas alto = frena mas seco
const TURN_DAMPING: f32 = 6.0;
// subir y bajar la nariz va mas lento q girar a los lados, esta es la proporcion
const PITCH_SCALE: f32 = 0.7;
// hasta donde puede subir o bajar la nariz, asi nunca queda viendo recto pa arriba
const PITCH_LIMIT: f32 = 1.4;
// lo maximo q se ladea la nave en una curva (radianes) y q tan rapido llega a eso
const ROLL_MAX: f32 = 0.5;
const ROLL_RESPONSE: f32 = 6.0;

// --- gravedad ---
// lo maximo q puede jalar la gravedad sumando todos los cuerpos
// el empuje normal de la nave es 20, mientras esto sea menor siempre se puede escapar
const MAX_GRAVITY_ACCEL: f32 = 12.0;

// --- modo orbita ---
// empuje con w/s mientras rodeamos el planeta
const ORBIT_ACCEL: f32 = 6.0;
// freno normal y freno con espacio, con estos numeros la velocidad maxima es 6 / 0.8 = 7.5
// asi una vuelta tarda como 7 segundos y da tiempo de ver la superficie
const ORBIT_DRAG: f32 = 0.8;
const ORBIT_BRAKE_DRAG: f32 = 3.0;
// q tan rapido sube y baja con las flechas
const ORBIT_CLIMB_ACCEL: f32 = 12.0;
const ORBIT_CLIMB_MAX: f32 = 6.0;
// lo mas bajo q se puede volar, medido encima de donde empieza la colision
const ORBIT_MIN_ALT: f32 = 0.3;
// lo mas alto q se puede volar, en radios del planeta
const ORBIT_MAX_ALT_RADII: f32 = 3.0;
// q tan rapido nos saca hacia arriba si enganchamos muy pegados a la superficie
const ORBIT_ALT_RESPONSE: f32 = 2.0;
// q tan rapido cambia la nave entre la orientacion de vuelo y la de orbita, pa q no de un salto
const ORBIT_BLEND_RESPONSE: f32 = 4.0;
// en orbita la camara mira un poco hacia abajo pa ver la superficie, mas alto = mas inclinada
const ORBIT_CAMERA_TILT: f32 = 3.0;

// --- warp ---
// velocidad de crucero y con cuanta aceleracion llega a ella (tarda 1 segundo)
const WARP_SPEED: f32 = 90.0;
const WARP_ACCEL: f32 = 90.0;
// cuanto dura la fase de alinearse antes de salir
const WARP_ALIGN_TIME: f32 = 1.0;
// q tan rapido apunta al destino mientras se alinea y mientras viaja
const WARP_ALIGN_RESPONSE: f32 = 5.0;
const WARP_STEER_RESPONSE: f32 = 6.0;
// empieza a frenar a esta distancia del centro del destino y termina a esta otra, en radios
// a 2 radios del centro queda bien adentro del rango de la f, asi da tiempo de apretarla
// antes de q el planeta se aleje
const WARP_BRAKE_RADII: f32 = 6.0;
const WARP_ARRIVE_RADII: f32 = 2.0;
// cuanto margen deja al pasar junto a otro cuerpo, encima de su colision
const WARP_CLEARANCE: f32 = 6.0;
// por seguridad, si un viaje tarda mas q esto se corta
const WARP_MAX_TIME: f32 = 20.0;
// si cancelan el viaje la nave queda con esta velocidad como maximo, pa poder controlarla
const WARP_CANCEL_SPEED: f32 = 25.0;
// q tan rapido prende y apaga el efecto visual
const WARP_EFFECT_RESPONSE: f32 = 3.0;

// las fases de un viaje, en orden
#[derive(Clone, Copy, PartialEq, Debug)]
enum WarpPhase {
    Align,
    Accelerate,
    Cruise,
    Brake,
}

// un viaje en curso
struct WarpTrip {
    /// indice del mundo destino
    target: usize,
    phase: WarpPhase,
    /// cuanto lleva el viaje, en segundos
    elapsed: f32,
    /// a q velocidad va, en brake es relativa al planeta
    speed: f32,
}

// el radio donde empieza la colision de un cuerpo, la deformacion de los planetas llega hasta un 12%
pub fn collision_radius(radius: f32) -> f32 {
    radius * 1.12 + 0.8
}

// la velocidad de un cuerpo, comparando donde estaba hace un momentito
fn body_velocity(scene: &Scene, i: usize, t: f32) -> Vec3 {
    let moment = 0.01;
    (scene.body_position(i, t) - scene.body_position(i, t - moment)) / moment
}

// un vector cualquiera q forme angulo recto con v, pa cuando no hay otra referencia
fn any_perpendicular(v: Vec3) -> Vec3 {
    let axis = if v.y.abs() < 0.9 { Vec3::UP } else { Vec3::new(1.0, 0.0, 0.0) };
    v.cross(axis).normalized()
}

// como vamos enganchados a un planeta, todo medido desde su centro
struct Orbit {
    /// indice del cuerpo q estamos rodeando
    body: usize,
    /// de largo 1, del centro del planeta hacia la nave, tambien es el "arriba" de la nave
    normal: Vec3,
    /// de largo 1, pa donde apunta la nave, siempre pegado a la superficie (angulo recto con normal)
    heading: Vec3,
    /// a q altura va sobre la superficie
    altitude: f32,
    /// a q velocidad avanza rodeando el planeta, negativa si va de reversa
    speed: f32,
    /// a q velocidad va subiendo o bajando
    climb_rate: f32,
}

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
    /// a q velocidad esta girando a los lados ahorita (rad/s), positivo = a la derecha
    pub yaw_rate: f32,
    /// a q velocidad esta subiendo o bajando la nariz ahorita (rad/s), positivo = pa arriba
    pub pitch_rate: f32,
    /// si los cuerpos atraen a la nave o no (tecla G), el hud lo lee pa mostrarlo
    pub gravity_on: bool,
    // la orbita actual, None = vuelo normal
    orbit: Option<Orbit>,
    // de 0 a 1: 0 = orientacion de vuelo, 1 = orientacion de orbita, en medio = cambiando suave
    frame_blend: f32,
    // la ultima orientacion de orbita, se guarda pa poder regresar suave despues de soltarla
    orbit_forward: Vec3,
    orbit_up: Vec3,
    // el viaje actual, None = sin viaje
    warp_trip: Option<WarpTrip>,
    // intensidad del efecto visual, de 0 a 1
    warp_effect: f32,
}

// calcula la nueva velocidad de giro de un eje, sirve igual pa yaw y pa pitch
fn next_spin_rate(rate: f32, input: f32, accel: f32, max: f32, dt: f32) -> f32 {
    if input != 0.0 {
        // con la tecla apretada el giro va subiendo poco a poco, y lo topamos en el maximo
        (rate + input * accel * dt).clamp(-max, max)
    } else {
        // sin tecla se va frenando solo, con exp pa q frene igual a cualquier fps
        rate * (-TURN_DAMPING * dt).exp()
    }
}

/// Lo que el jugador presiona este cuadro (main.rs lo llena desde raylib).
#[derive(Default, Clone, Copy)]
pub struct ShipInput {
    pub thrust: f32, // -1..1  (W / S)
    pub turn: f32,   // -1..1  (A / D)
    pub climb: f32,  // -1..1  (flechas arriba/abajo)
    pub boost: bool, // Shift
    pub brake: bool, // Espacio
    // estas son de un toque: solo valen true en el cuadro en q se aprieta la tecla
    pub orbit_toggle: bool,    // F
    pub scan: bool,            // E
    pub warp: bool,            // Tab
    pub select: Option<usize>, // teclas 1 a 5, 1 = bodies[1]
    pub photo: bool,           // P
    pub gravity_toggle: bool,  // G
}

impl Ship {
    pub fn new(position: Vec3) -> Ship {
        Ship { position, velocity: Vec3::ZERO, yaw: 0.0, pitch: 0.0, roll: 0.0, boost: 0.0, yaw_rate: 0.0, pitch_rate: 0.0, gravity_on: true, orbit: None, frame_blend: 0.0, orbit_forward: Vec3::ZERO, orbit_up: Vec3::UP, warp_trip: None, warp_effect: 0.0 }
    }

    pub fn forward(&self) -> Vec3 {
        Vec3::new(-self.yaw.sin() * self.pitch.cos(), self.pitch.sin(), -self.yaw.cos() * self.pitch.cos())
    }

    /// Base ortonormal (derecha, arriba, adelante) de la nave.
    pub fn basis(&self) -> (Vec3, Vec3, Vec3) {
        let (f, up) = self.frame();
        let mut r = f.cross(up).normalized();
        // si adelante y arriba quedaron alineados no hay derecha definida, agarramos cualquiera
        if r.length() < 0.5 {
            r = any_perpendicular(f);
        }
        let u = r.cross(f);
        (r, u, f)
    }

    // pa donde mira la nave y cual es su "arriba de referencia" en este momento
    fn frame(&self) -> (Vec3, Vec3) {
        // en vuelo normal arriba siempre es +y, esto es exactamente lo de siempre
        if self.frame_blend <= 0.0 {
            return (self.forward(), Vec3::UP);
        }
        // cambiando de modo o en orbita mezclamos las dos orientaciones segun el blend
        let mut f = self.forward().lerp(self.orbit_forward, self.frame_blend).normalized();
        if f.length() < 0.5 {
            f = self.orbit_forward;
        }
        (f, Vec3::UP.lerp(self.orbit_up, self.frame_blend).normalized())
    }

    /// el "arriba" q debe usar la camara: +y volando normal, la normal del planeta en orbita
    pub fn camera_up(&self) -> Vec3 {
        let (_, up) = self.frame();
        // en el polo sur mezclar +y con -y puede dar cero justo a la mitad
        if up.length() < 0.5 {
            return self.basis().1;
        }
        up
    }

    /// el cuerpo al q estamos enganchados, si hay alguno
    pub fn orbit_body(&self) -> Option<usize> {
        self.orbit.as_ref().map(|o| o.body)
    }

    /// se engancha al cuerpo i desde donde este la nave ahorita, sin mover nada de golpe
    pub fn engage_orbit(&mut self, i: usize, scene: &Scene, t: f32) {
        // paso 1: el "arriba" es la linea q va del centro del planeta a la nave
        let radius = scene.bodies[i].radius;
        let center = scene.body_position(i, t);
        let from_center = self.position - center;
        let distance = from_center.length().max(1e-4);
        let normal = from_center / distance;

        // paso 2: a lo q mira la nave le quito la parte q apunta hacia o contra el planeta
        // lo q queda es la direccion pegada a la superficie
        let (_, _, looking) = self.basis();
        let flat = looking - normal * looking.dot(normal);
        // si iba mirando recto al planeta no queda casi nada, agarramos cualquier direccion valida
        let heading = if flat.length() > 0.1 { flat.normalized() } else { any_perpendicular(normal) };

        // paso 3: la velocidad con la q vamos respecto al planeta, q tambien se esta moviendo
        let relative = self.velocity - body_velocity(scene, i, t);

        // paso 4: la orbita arranca con el movimiento q ya traia, asi no se siente un frenon
        self.orbit = Some(Orbit {
            body: i,
            normal,
            heading,
            altitude: distance - radius,
            speed: relative.dot(heading),
            climb_rate: relative.dot(normal).clamp(-ORBIT_CLIMB_MAX, ORBIT_CLIMB_MAX),
        });
        self.orbit_forward = heading;
        self.orbit_up = normal;
        self.pitch_rate = 0.0;
    }

    /// suelta la orbita, la nave sigue con la velocidad q traia
    pub fn release_orbit(&mut self) {
        if self.orbit.take().is_none() {
            return;
        }
        // el vuelo normal usa yaw y pitch, asi q los saco de pa donde estaba mirando
        let f = self.orbit_forward;
        self.yaw = (-f.x).atan2(-f.z);
        self.pitch = f.y.asin().clamp(-PITCH_LIMIT, PITCH_LIMIT);
    }

    // un cuadro rodeando el planeta
    fn orbit_step(&mut self, input: ShipInput, dt: f32, scene: &Scene, t: f32) {
        let Some(o) = self.orbit.as_mut() else { return };
        let radius = scene.bodies[o.body].radius;

        // paso 1: a/d giran el rumbo sobre la superficie, con la misma inercia del vuelo normal
        // mezclo el rumbo con su derecha: eso lo rota sin despegarlo de la superficie
        let old_yaw_rate = self.yaw_rate;
        self.yaw_rate = next_spin_rate(self.yaw_rate, input.turn, TURN_ACCEL, TURN_MAX, dt);
        let turn = (old_yaw_rate + self.yaw_rate) * 0.5 * dt;
        let right = o.heading.cross(o.normal);
        o.heading = (o.heading * turn.cos() + right * turn.sin()).normalized();

        // paso 2: w/s aceleran alrededor del planeta y siempre hay un poco de freno
        // con empuje y freno juntos la velocidad se acerca sola a empuje / freno y ahi se queda
        // esta formula da lo mismo con cualquier dt, sin importar cuantos cuadros se dibujen
        let old_speed = o.speed;
        let drag = if input.brake { ORBIT_BRAKE_DRAG } else { ORBIT_DRAG };
        let top_speed = input.thrust * ORBIT_ACCEL / drag;
        o.speed = top_speed + (o.speed - top_speed) * (-drag * dt).exp();

        // paso 3: avanzar rodeando es rotar normal y rumbo juntos, un angulo = distancia / radio
        let angle = (old_speed + o.speed) * 0.5 * dt / (radius + o.altitude);
        let (normal, heading) = (o.normal, o.heading);
        o.normal = (normal * angle.cos() + heading * angle.sin()).normalized();
        o.heading = (heading * angle.cos() - normal * angle.sin()).normalized();
        // los redondeos los van separando de a poquito, asi los regreso al angulo recto
        o.heading = (o.heading - o.normal * o.heading.dot(o.normal)).normalized();

        // paso 4: las flechas cambian la altura
        let old_climb = o.climb_rate;
        o.climb_rate = next_spin_rate(o.climb_rate, input.climb, ORBIT_CLIMB_ACCEL, ORBIT_CLIMB_MAX, dt);
        o.altitude += (old_climb + o.climb_rate) * 0.5 * dt;

        // paso 5: los limites de altura, el de abajo empuja suave por si enganchamos muy pegados
        let min_altitude = collision_radius(radius) - radius + ORBIT_MIN_ALT;
        let max_altitude = radius * ORBIT_MAX_ALT_RADII;
        if o.altitude < min_altitude {
            o.altitude += (min_altitude - o.altitude) * (1.0 - (-ORBIT_ALT_RESPONSE * dt).exp());
            o.climb_rate = o.climb_rate.max(0.0);
        }
        if o.altitude > max_altitude {
            o.altitude = max_altitude;
            o.climb_rate = o.climb_rate.min(0.0);
        }

        // paso 6: la posicion sale del centro del planeta en este momento
        // por eso la nave lo sigue solita mientras el planeta orbita al sol
        let old_position = self.position;
        self.position = scene.body_position(o.body, t) + o.normal * (radius + o.altitude);
        self.orbit_forward = o.heading;
        self.orbit_up = o.normal;

        // paso 7: la velocidad real es lo q nos movimos entre el tiempo, se usa al soltar la orbita
        self.velocity = (self.position - old_position) / dt.max(1e-5);

        self.update_visuals(false, dt);
    }

    /// si hay un viaje en curso
    pub fn warping(&self) -> bool {
        self.warp_trip.is_some()
    }

    /// intensidad del efecto visual del warp, de 0 a 1
    pub fn warp_effect(&self) -> f32 {
        self.warp_effect
    }

    /// empieza un viaje al mundo i, si ya estamos cerca de ese mundo no hace nada
    pub fn start_warp(&mut self, i: usize, scene: &Scene, t: f32) {
        let distance = (scene.body_position(i, t) - self.position).length();
        if distance < WARP_BRAKE_RADII * scene.bodies[i].radius {
            return;
        }
        // si veniamos en orbita la soltamos, el viaje necesita manejar la nave
        self.release_orbit();
        self.yaw_rate = 0.0;
        self.pitch_rate = 0.0;
        self.warp_trip = Some(WarpTrip { target: i, phase: WarpPhase::Align, elapsed: 0.0, speed: 0.0 });
    }

    /// corta el viaje donde vaya, la nave sigue volando pero a una velocidad q se pueda controlar
    pub fn cancel_warp(&mut self) {
        self.warp_trip = None;
        let speed = self.velocity.length();
        if speed > WARP_CANCEL_SPEED {
            self.velocity = self.velocity / speed * WARP_CANCEL_SPEED;
        }
    }

    // a q punto hay q apuntar ahorita pa llegar al destino sin chocar
    fn warp_aim(&self, target: usize, scene: &Scene, t: f32) -> Vec3 {
        // paso 1: el planeta se mueve, asi q apuntamos a donde va a estar cuando lleguemos
        // el tiempo q falta es la distancia entre la velocidad de crucero, y se recalcula cada cuadro
        let eta = (scene.body_position(target, t) - self.position).length() / WARP_SPEED;
        let aim = scene.body_position(target, t + eta);
        let path = aim - self.position;
        let length = path.length().max(1e-4);
        let dir = path / length;

        // paso 2: buscar el primer cuerpo q nos quede en medio del camino
        let mut waypoint = aim;
        let mut first_along_path = f32::MAX;
        for (j, b) in scene.bodies.iter().enumerate() {
            if j == target {
                continue;
            }
            // cuanto avanzamos por el camino hasta quedar a la par del cuerpo
            let center = scene.body_position(j, t);
            let along = (center - self.position).dot(dir);
            // si queda atras o despues del destino no estorba
            if along <= 0.0 || along >= length || along >= first_along_path {
                continue;
            }
            // lo mas cerca q pasariamos del cuerpo yendo recto
            let closest = self.position + dir * along;
            let away = closest - center;
            let clearance = collision_radius(b.radius) + WARP_CLEARANCE;
            if away.length() < clearance {
                // paso 3: en vez de ir recto, apuntamos a un punto al lado del cuerpo
                // si vamos recto contra su centro no hay lado, entonces agarramos cualquiera
                let side = if away.length() > 0.01 { away.normalized() } else { any_perpendicular(dir) };
                waypoint = center + side * clearance;
                first_along_path = along;
            }
        }
        waypoint
    }

    // gira la nariz hacia una direccion, suave y sin depender de los fps
    fn steer_toward(&mut self, direction: Vec3, response: f32, dt: f32) {
        let d = direction.normalized();
        let k = 1.0 - (-response * dt).exp();
        // la vuelta mas corta: si la diferencia da mas de media vuelta mejor gira pal otro lado
        let pi = std::f32::consts::PI;
        let yaw_difference = ((-d.x).atan2(-d.z) - self.yaw + pi).rem_euclid(2.0 * pi) - pi;
        self.yaw += yaw_difference * k;
        let wanted_pitch = d.y.clamp(-1.0, 1.0).asin().clamp(-PITCH_LIMIT, PITCH_LIMIT);
        self.pitch += (wanted_pitch - self.pitch) * k;
    }

    // un cuadro de viaje
    fn warp_step(&mut self, dt: f32, scene: &Scene, t: f32) {
        let Some(mut trip) = self.warp_trip.take() else { return };
        trip.elapsed += dt;
        let radius = scene.bodies[trip.target].radius;
        let center = scene.body_position(trip.target, t);
        let distance = (center - self.position).length();
        let aim = self.warp_aim(trip.target, scene, t);
        let to_aim = aim - self.position;
        let mut finished = trip.elapsed > WARP_MAX_TIME;

        match trip.phase {
            WarpPhase::Align => {
                // se va frenando lo q traia mientras gira hacia el destino
                self.steer_toward(to_aim, WARP_ALIGN_RESPONSE, dt);
                self.velocity = self.velocity * (-3.0 * dt).exp();
                if trip.elapsed >= WARP_ALIGN_TIME {
                    trip.speed = self.velocity.dot(self.forward()).max(0.0);
                    trip.phase = WarpPhase::Accelerate;
                }
            }
            WarpPhase::Accelerate | WarpPhase::Cruise => {
                self.steer_toward(to_aim, WARP_STEER_RESPONSE, dt);
                trip.speed = (trip.speed + WARP_ACCEL * dt).min(WARP_SPEED);
                if trip.speed >= WARP_SPEED {
                    trip.phase = WarpPhase::Cruise;
                }
                if distance < WARP_BRAKE_RADII * radius {
                    trip.phase = WarpPhase::Brake;
                }
                self.velocity = self.forward() * trip.speed;
            }
            WarpPhase::Brake => {
                self.steer_toward(to_aim, WARP_STEER_RESPONSE, dt);
                // entre menos falta para llegar mas lento va, asi frena suave y nunca se pasa
                // si venia lento porque el viaje era corto, puede acelerar hasta esa velocidad
                let left = distance - WARP_ARRIVE_RADII * radius;
                let per_second = WARP_SPEED / ((WARP_BRAKE_RADII - WARP_ARRIVE_RADII) * radius);
                trip.speed = (trip.speed + WARP_ACCEL * dt).min(left * per_second).max(0.0);
                // la velocidad del planeta se suma, asi vamos llegando a su mismo paso
                let planet_velocity = body_velocity(scene, trip.target, t);
                self.velocity = self.forward() * trip.speed + planet_velocity;
                if left < 0.5 {
                    self.velocity = planet_velocity;
                    finished = true;
                }
            }
        }
        self.position += self.velocity * dt;

        // la ruta ya esquiva todo, pero si un cuerpo igual nos queda encima cortamos el viaje
        let mut hit = false;
        if trip.phase != WarpPhase::Align {
            for (j, b) in scene.bodies.iter().enumerate() {
                let to_body = scene.body_position(j, t) - self.position;
                if to_body.length() < collision_radius(b.radius) && self.velocity.dot(to_body) > 0.0 {
                    hit = true;
                }
            }
        }

        let flame = matches!(trip.phase, WarpPhase::Accelerate | WarpPhase::Cruise);
        self.update_visuals(flame, dt);
        if hit {
            self.cancel_warp();
        } else if !finished {
            self.warp_trip = Some(trip);
        }
    }

    // ladeo y turbo, se usan igual en vuelo normal y en orbita
    fn update_visuals(&mut self, boost: bool, dt: f32) {
        // la nave se ladea segun lo q de verdad esta girando, no segun la tecla
        // por eso al soltar se endereza junto con el giro
        let target_roll = -self.yaw_rate / TURN_MAX * ROLL_MAX;
        self.roll += (target_roll - self.roll) * (1.0 - (-ROLL_RESPONSE * dt).exp());

        // Turbo: sube y baja suave (independiente de los FPS). Enciende rápido y se apaga
        // un poco más lento, como si la llama se fuera consumiendo.
        let boost_target = if boost { 1.0 } else { 0.0 };
        let boost_rate = if boost_target > self.boost { 8.0 } else { 3.0 };
        self.boost += (boost_target - self.boost) * (1.0 - (-boost_rate * dt).exp());
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
        // la g solo cambia el interruptor, no toca la velocidad q ya traia la nave
        if input.gravity_toggle {
            self.gravity_on = !self.gravity_on;
        }
        if self.warp_trip.is_some() {
            // en un viaje la nave va sola, las teclas de vuelo no hacen nada
            self.warp_step(dt, scene, t);
        } else if self.orbit.is_some() {
            // en orbita la altura ya la controlamos nosotros, por eso aqui no hay gravedad
            self.orbit_step(input, dt, scene, t);
        } else {
            if self.gravity_on {
                self.velocity += self.gravity_accel(scene, t) * dt;
            }
            self.fly(input, dt);
        }
        self.collide(scene, t);

        // el efecto visual solo va prendido mientras la nave va acelerando o en crucero
        let at_speed = matches!(self.warp_trip.as_ref().map(|trip| trip.phase), Some(WarpPhase::Accelerate | WarpPhase::Cruise));
        let effect_target = if at_speed { 1.0 } else { 0.0 };
        self.warp_effect += (effect_target - self.warp_effect) * (1.0 - (-WARP_EFFECT_RESPONSE * dt).exp());
        if self.warp_effect < 0.001 && !at_speed {
            self.warp_effect = 0.0;
        }

        // el blend camina hacia 1 en orbita y hacia 0 en vuelo, y al llegar se queda exacto
        let target = if self.orbit.is_some() { 1.0 } else { 0.0 };
        self.frame_blend += (target - self.frame_blend) * (1.0 - (-ORBIT_BLEND_RESPONSE * dt).exp());
        if (target - self.frame_blend).abs() < 0.001 {
            self.frame_blend = target;
        }
    }

    // cuanto y pa donde jalan todos los cuerpos a la nave en este momento
    fn gravity_accel(&self, scene: &Scene, t: f32) -> Vec3 {
        let mut total = Vec3::ZERO;
        for (i, b) in scene.bodies.iter().enumerate() {
            // paso 1: la flecha de la nave al cuerpo y q tan lejos esta
            let to_body = scene.body_position(i, t) - self.position;
            let d = to_body.length().max(1e-4);

            // paso 2: entre mas lejos, jala mucho menos (a = gravity / d²)
            // abajo de la superficie ya no sube mas, si no cerca del centro seria infinito
            let strength = b.gravity / (d * d).max(b.radius * b.radius);

            // paso 3: to_body / d es solo la direccion, de largo 1
            total += to_body / d * strength;
        }

        // paso 4: si entre todos se pasan del tope, dejo la misma direccion pero recorto el largo
        let size = total.length();
        if size > MAX_GRAVITY_ACCEL {
            total = total / size * MAX_GRAVITY_ACCEL;
        }
        total
    }

    // todo el movimiento de la nave en un cuadro, lo separe de la colision pa poder probarlo sin escena
    fn fly(&mut self, input: ShipInput, dt: f32) {
        // paso 1: guardo los giros de antes pa poder promediarlos ahorita
        let (old_yaw_rate, old_pitch_rate) = (self.yaw_rate, self.pitch_rate);

        // paso 2: actualizo q tan rapido esta girando la nave en cada eje
        self.yaw_rate = next_spin_rate(self.yaw_rate, input.turn, TURN_ACCEL, TURN_MAX, dt);
        self.pitch_rate = next_spin_rate(self.pitch_rate, input.climb, TURN_ACCEL * PITCH_SCALE, TURN_MAX * PITCH_SCALE, dt);

        // paso 3: muevo los angulos con el promedio del giro de antes y el de ahora
        // asi el resultado casi no cambia entre una compu de 30 fps y una de 144
        self.yaw -= (old_yaw_rate + self.yaw_rate) * 0.5 * dt;
        self.pitch += (old_pitch_rate + self.pitch_rate) * 0.5 * dt;

        // paso 4: si la nariz llego al tope paro el giro de ese eje, si no se queda "cargado"
        // y al soltar tardaria en responder pal otro lado
        if self.pitch.abs() > PITCH_LIMIT {
            self.pitch = self.pitch.clamp(-PITCH_LIMIT, PITCH_LIMIT);
            self.pitch_rate = 0.0;
        }

        // paso 5: ladeo y llama del turbo
        self.update_visuals(input.boost, dt);

        // Aceleración progresiva hacia adelante + fricción exponencial.
        // El turbo (Shift) empuja hacia adelante aunque no se presione W.
        let thrust = if input.boost { 1.0 } else { input.thrust };
        let accel = if input.boost { 60.0 } else { 20.0 };
        self.velocity += self.forward() * (thrust * accel * dt);

        let drag = if input.brake { 3.0 } else { 0.6 };
        self.velocity = self.velocity * (-drag * dt).exp();
        self.position += self.velocity * dt;
    }

    fn collide(&mut self, scene: &Scene, t: f32) {
        // Colisión básica: no atravesar planetas ni el sol (empujar hacia afuera).
        for (i, b) in scene.bodies.iter().enumerate() {
            let center = scene.body_position(i, t);
            let to_ship = self.position - center;
            let min_dist = collision_radius(b.radius);
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
    /// el "arriba" de la camara, viene de la nave pa q funcione tambien en los polos de un planeta
    pub up: Vec3,
}

impl ChaseCamera {
    pub fn new(ship: &Ship) -> ChaseCamera {
        let mut c = ChaseCamera { position: ship.position, target: ship.position, distance: 7.0, height: 2.2, up: Vec3::UP };
        c.update(ship, 1.0);
        c
    }

    pub fn update(&mut self, ship: &Ship, dt: f32) {
        let (_, up, f) = ship.basis();
        let desired = ship.position - f * self.distance + up * self.height;
        let k = 1.0 - (-8.0 * dt).exp(); // suavizado independiente de los FPS
        self.position = self.position.lerp(desired, k);
        // volando normal el blend es 0 y esto es lo de siempre, en orbita baja la mirada al planeta
        self.target = ship.position + f * 6.0 - up * (ORBIT_CAMERA_TILT * ship.frame_blend);
        self.up = ship.camera_up();
    }

    pub fn view(&self) -> Mat4 {
        Mat4::look_at(self.position, self.target, self.up)
    }

    /// (derecha, arriba, adelante) de la cámara.
    pub fn basis(&self) -> (Vec3, Vec3, Vec3) {
        let f = (self.target - self.position).normalized();
        let r = f.cross(self.up).normalized();
        (r, r.cross(f), f)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // el recorrido de prueba: q teclas van apretadas segun el segundo en q vamos
    // depende del tiempo y no del numero de cuadro, igual q cuando alguien juega de verdad
    fn route_input(t: f32) -> ShipInput {
        let mut input = ShipInput::default();
        if t < 1.0 {
            input.thrust = 1.0;
            input.turn = 1.0;
        } else if t < 2.0 {
            input.thrust = 1.0;
        } else if t < 3.0 {
            input.thrust = 1.0;
            input.climb = 1.0;
        } else if t < 4.0 {
            input.boost = true;
            input.turn = -1.0;
        }
        // de 4 a 6 segundos no se aprieta nada, la nave sigue por inercia
        input
    }

    // vuela el recorrido completo de 6 segundos con un dt fijo
    fn fly_route(dt: f32) -> Ship {
        let mut ship = Ship::new(Vec3::ZERO);
        let steps = (6.0 / dt).round() as usize;
        for i in 0..steps {
            // uso la mitad del cuadro pa q un redondeo no cambie de tramo un cuadro antes
            let t = (i as f32 + 0.5) * dt;
            ship.fly(route_input(t), dt);
        }
        ship
    }

    #[test]
    fn el_recorrido_casi_no_cambia_con_los_fps() {
        let slow = fly_route(1.0 / 30.0);
        let fast = fly_route(1.0 / 144.0);
        let pos_diff = (slow.position - fast.position).length();
        let traveled = fast.position.length();
        println!("30 fps : pos=({:.2}, {:.2}, {:.2}) yaw={:.4} pitch={:.4}", slow.position.x, slow.position.y, slow.position.z, slow.yaw, slow.pitch);
        println!("144 fps: pos=({:.2}, {:.2}, {:.2}) yaw={:.4} pitch={:.4}", fast.position.x, fast.position.y, fast.position.z, fast.yaw, fast.pitch);
        println!("diferencia: {:.3} unidades de {:.1} desde el inicio ({:.2}%)", pos_diff, traveled, pos_diff / traveled * 100.0);
        println!("yaw: {:.5} rad, pitch: {:.5} rad", (slow.yaw - fast.yaw).abs(), (slow.pitch - fast.pitch).abs());
        assert!(pos_diff / traveled < 0.02);
        assert!((slow.yaw - fast.yaw).abs() < 0.01);
        assert!((slow.pitch - fast.pitch).abs() < 0.01);
    }

    #[test]
    fn el_giro_sube_poco_a_poco_y_se_frena_al_soltar() {
        let dt = 1.0 / 60.0;
        let mut ship = Ship::new(Vec3::ZERO);
        let right = ShipInput { turn: 1.0, ..Default::default() };

        // en el primer cuadro todavia no puede ir a toda velocidad
        ship.fly(right, dt);
        assert!(ship.yaw_rate > 0.0 && ship.yaw_rate < TURN_MAX * 0.1);

        // con la tecla apretada un buen rato llega al maximo y no se pasa
        for _ in 0..120 {
            ship.fly(right, dt);
        }
        assert!((ship.yaw_rate - TURN_MAX).abs() < 1e-4);

        // al soltar sigue girando un poquito y despues casi se detiene
        ship.fly(ShipInput::default(), dt);
        assert!(ship.yaw_rate > 0.0 && ship.yaw_rate < TURN_MAX);
        for _ in 0..120 {
            ship.fly(ShipInput::default(), dt);
        }
        assert!(ship.yaw_rate.abs() < 0.01);
    }

    // simula a la nave cayendo y devuelve las distancias al centro del planeta: inicio, final y la menor
    fn drop_ship(scene: &Scene, i: usize, start_height: f32, seconds: f32) -> (f32, f32, f32) {
        let dt = 1.0 / 60.0;
        let up = Vec3::new(0.0, scene.bodies[i].radius + start_height, 0.0);
        let mut ship = Ship::new(scene.body_position(i, 0.0) + up);
        let start = (ship.position - scene.body_position(i, 0.0)).length();
        let mut closest = start;
        let mut last = start;
        for step in 0..(seconds / dt).round() as usize {
            let t = step as f32 * dt;
            ship.update(ShipInput::default(), dt, scene, t);
            last = (ship.position - scene.body_position(i, t)).length();
            closest = closest.min(last);
        }
        (start, last, closest)
    }

    #[test]
    fn la_nave_quieta_cae_y_la_colision_la_detiene() {
        let mut scene = Scene::new();
        // freno la orbita del planeta pa ver solo la gravedad, si no se aleja de la nave
        scene.bodies[1].orbit_speed = 0.0;
        let floor = scene.bodies[1].radius * 1.12 + 0.8;

        let (start, after_3s, _) = drop_ship(&scene, 1, 3.0, 3.0);
        println!("en 3 s: distancia al centro de {start:.2} a {after_3s:.2} (la colision esta en {floor:.2})");
        assert!(after_3s < start);

        // con mas tiempo llega hasta la colision y nunca la atraviesa
        let (_, after_20s, closest) = drop_ship(&scene, 1, 3.0, 20.0);
        println!("en 20 s: distancia {after_20s:.2}, la menor fue {closest:.2}");
        assert!(closest >= floor - 1e-3);
        assert!((after_20s - floor).abs() < 0.05);
    }

    #[test]
    fn la_g_apaga_la_gravedad() {
        let scene = Scene::new();
        let dt = 1.0 / 60.0;
        let center = scene.body_position(1, 0.0);
        let mut ship = Ship::new(center + Vec3::new(0.0, 10.0, 0.0));

        // el cuadro en q se aprieta ya no jala nada, y sigue apagada aunque ya se solto
        ship.update(ShipInput { gravity_toggle: true, ..Default::default() }, dt, &scene, 0.0);
        ship.update(ShipInput::default(), dt, &scene, dt);
        assert!(!ship.gravity_on);
        assert_eq!(ship.velocity.length(), 0.0);

        // otra vez la g y vuelve a jalar
        ship.update(ShipInput { gravity_toggle: true, ..Default::default() }, dt, &scene, dt * 2.0);
        assert!(ship.gravity_on);
        assert!(ship.velocity.length() > 0.0);
    }

    #[test]
    fn siempre_se_puede_escapar_aunque_la_gravedad_sea_enorme() {
        let mut scene = Scene::new();
        scene.bodies[1].orbit_speed = 0.0;
        scene.bodies[1].gravity = 1_000_000.0;
        let center = scene.body_position(1, 0.0);
        let dt = 1.0 / 60.0;

        // la nave mira a -z, asi q la pongo de ese lado pa q acelerar sea alejarse
        let mut ship = Ship::new(center + Vec3::new(0.0, 0.0, -(scene.bodies[1].radius + 2.0)));
        assert!(ship.gravity_accel(&scene, 0.0).length() <= MAX_GRAVITY_ACCEL + 1e-3);

        let start = (ship.position - center).length();
        for step in 0..120 {
            ship.update(ShipInput { thrust: 1.0, ..Default::default() }, dt, &scene, step as f32 * dt);
        }
        let end = (ship.position - center).length();
        println!("escapando con gravedad enorme: de {start:.2} a {end:.2} en 2 s");
        assert!(end > start + 2.0);
    }

    // pone la nave a cierta altura en una direccion del planeta y la engancha, sin velocidad de arranque
    fn orbiting_ship(scene: &Scene, i: usize, dir: Vec3, height: f32) -> Ship {
        let center = scene.body_position(i, 0.0);
        let mut ship = Ship::new(center + dir * (scene.bodies[i].radius + height));
        ship.engage_orbit(i, scene, 0.0);
        let o = ship.orbit.as_mut().unwrap();
        o.speed = 0.0;
        o.climb_rate = 0.0;
        ship
    }

    fn is_finite(v: Vec3) -> bool {
        v.x.is_finite() && v.y.is_finite() && v.z.is_finite()
    }

    // revisa q la orientacion de la nave sea valida: sin nan y con los 3 ejes de largo 1
    fn assert_valid_basis(ship: &Ship) {
        let (r, u, f) = ship.basis();
        for v in [r, u, f] {
            assert!(is_finite(v));
            assert!((v.length() - 1.0).abs() < 0.01);
        }
        assert!(is_finite(ship.camera_up()));
    }

    #[test]
    fn la_orbita_sigue_al_planeta_a_la_misma_altura() {
        let scene = Scene::new();
        let dt = 1.0 / 60.0;
        let mut ship = orbiting_ship(&scene, 1, Vec3::UP, 3.0);
        let wanted = scene.bodies[1].radius + 3.0;
        let start_position = ship.position;

        // en 10 s el planeta avanza como 36 unidades, y la nave se tiene q ir con el
        for step in 1..=600 {
            let t = step as f32 * dt;
            ship.update(ShipInput::default(), dt, &scene, t);
            let distance = (ship.position - scene.body_position(1, t)).length();
            assert!((distance - wanted).abs() < 0.01);
        }
        assert!((ship.position - start_position).length() > 20.0);
    }

    #[test]
    fn enganchar_no_mueve_la_nave_de_golpe() {
        let scene = Scene::new();
        let dt = 1.0 / 60.0;
        let mut ship = Ship::new(scene.body_position(1, 0.0) + Vec3::new(3.0, 5.0, 2.0));
        let before = ship.position;
        let (_, _, looking_before) = ship.basis();

        ship.engage_orbit(1, &scene, 0.0);
        ship.update(ShipInput::default(), dt, &scene, dt);

        // en un cuadro solo se puede mover lo q se movio el planeta
        assert!((ship.position - before).length() < 0.2);
        // y la orientacion todavia casi no cambio
        let (_, _, looking_after) = ship.basis();
        assert!(looking_before.dot(looking_after) > 0.99);
    }

    #[test]
    fn w_rodea_el_planeta_y_la_nave_queda_bien_orientada() {
        let scene = Scene::new();
        let dt = 1.0 / 60.0;
        let mut ship = orbiting_ship(&scene, 2, Vec3::new(1.0, 0.0, 0.0), 3.0);
        let start_normal = ship.orbit.as_ref().unwrap().normal;
        let accelerate = ShipInput { thrust: 1.0, ..Default::default() };

        // el primer cuadro la velocidad todavia es chiquita, sube poco a poco
        ship.update(accelerate, dt, &scene, dt);
        assert!(ship.orbit.as_ref().unwrap().speed < 1.0);

        // en 3 s da como un cuarto de vuelta, mas tiempo ya daria la vuelta completa y regresaria
        for step in 2..=180 {
            ship.update(accelerate, dt, &scene, step as f32 * dt);
        }
        let o = ship.orbit.as_ref().unwrap();
        assert!(o.normal.dot(start_normal) < 0.5, "no dio suficiente vuelta");
        assert!(o.heading.dot(o.normal).abs() < 1e-3, "el rumbo se despego de la superficie");

        // ya sin mezcla: el arriba de la nave es la normal y mira pa donde va
        let (_, up, forward) = ship.basis();
        assert_eq!(ship.frame_blend, 1.0);
        assert!(up.dot(o.normal) > 0.999);
        assert!(forward.dot(o.heading) > 0.999);
    }

    #[test]
    fn la_altura_respeta_los_limites_y_nunca_toca_la_colision() {
        let scene = Scene::new();
        let dt = 1.0 / 60.0;
        let radius = scene.bodies[2].radius;
        let mut ship = orbiting_ship(&scene, 2, Vec3::UP, 3.0);
        let mut step = 0;
        let mut run = |ship: &mut Ship, climb: f32| {
            for _ in 0..1200 {
                step += 1;
                ship.update(ShipInput { climb, ..Default::default() }, dt, &scene, step as f32 * dt);
            }
            ship.orbit.as_ref().unwrap().altitude
        };

        let top = run(&mut ship, 1.0);
        assert!((top - radius * ORBIT_MAX_ALT_RADII).abs() < 1e-3);

        let bottom = run(&mut ship, -1.0);
        assert!(bottom >= collision_radius(radius) - radius);
        println!("altura en krypton: de {bottom:.2} a {top:.2}");
    }

    #[test]
    fn al_soltar_conserva_la_velocidad_y_no_salta() {
        let scene = Scene::new();
        let dt = 1.0 / 60.0;
        let mut ship = orbiting_ship(&scene, 2, Vec3::new(1.0, 0.0, 0.0), 3.0);
        let mut t = 0.0;
        for _ in 0..180 {
            t += dt;
            ship.update(ShipInput { thrust: 1.0, ..Default::default() }, dt, &scene, t);
        }
        let (speed_before, position_before) = (ship.velocity, ship.position);
        let (_, _, looking_before) = ship.basis();
        assert!(speed_before.length() > 5.0);

        ship.release_orbit();
        assert_eq!(ship.orbit_body(), None);
        assert_eq!((ship.velocity - speed_before).length(), 0.0);

        // sin apretar nada sigue por donde iba, solo se mueve lo normal de un cuadro
        t += dt;
        ship.update(ShipInput::default(), dt, &scene, t);
        assert!((ship.position - position_before).length() < speed_before.length() * dt * 1.5);
        let (_, _, looking_after) = ship.basis();
        assert!(looking_before.dot(looking_after) > 0.99);
    }

    #[test]
    fn los_polos_no_rompen_la_nave_ni_la_camara() {
        let scene = Scene::new();
        let dt = 1.0 / 60.0;
        let south = Vec3::new(0.0, -1.0, 0.0);

        // arranco en cada polo, cruzo por encima un rato y suelto justo en el sur
        for dir in [Vec3::UP, south] {
            let mut ship = orbiting_ship(&scene, 2, dir, 3.0);
            let mut camera = ChaseCamera::new(&ship);
            let mut t = 0.0;
            for _ in 0..300 {
                t += dt;
                ship.update(ShipInput { thrust: 1.0, turn: 0.3, ..Default::default() }, dt, &scene, t);
                camera.update(&ship, dt);
                assert_valid_basis(&ship);
            }

            // lo devuelvo al polo sur y lo suelto, ahi +y y la normal son justo opuestos
            let center = scene.body_position(2, t);
            let o = ship.orbit.as_mut().unwrap();
            o.normal = south;
            o.heading = Vec3::new(1.0, 0.0, 0.0);
            ship.position = center + south * 8.0;
            ship.update(ShipInput::default(), dt, &scene, t + dt);
            ship.release_orbit();
            t += dt;

            // toda la vuelta al vuelo normal tiene q ser valida, cuadro por cuadro
            for _ in 0..300 {
                t += dt;
                ship.update(ShipInput::default(), dt, &scene, t);
                camera.update(&ship, dt);
                assert_valid_basis(&ship);
            }
            assert_eq!(ship.frame_blend, 0.0);
            assert_eq!(camera.up.y, 1.0);
        }
    }

    #[test]
    fn enganchar_mirando_recto_al_planeta_da_un_rumbo_valido() {
        let scene = Scene::new();
        let center = scene.body_position(1, 0.0);
        // la nave mira a -z, asi q la pongo en +z pa q vea justo al centro
        let mut ship = Ship::new(center + Vec3::new(0.0, 0.0, 8.0));
        ship.engage_orbit(1, &scene, 0.0);
        let o = ship.orbit.as_ref().unwrap();
        assert!(is_finite(o.heading));
        assert!((o.heading.length() - 1.0).abs() < 1e-3);
        assert!(o.heading.dot(o.normal).abs() < 1e-3);
    }

    #[test]
    fn la_orbita_casi_no_cambia_con_los_fps() {
        let scene = Scene::new();
        let run = |dt: f32| {
            let mut ship = orbiting_ship(&scene, 2, Vec3::new(1.0, 0.0, 0.0), 3.0);
            let steps = (6.0 / dt).round() as usize;
            for i in 1..=steps {
                let t = i as f32 * dt;
                let middle = t - dt * 0.5;
                let input = ShipInput { thrust: 1.0, turn: if middle < 2.0 { 1.0 } else { 0.0 }, climb: if middle < 1.0 { 1.0 } else { 0.0 }, ..Default::default() };
                ship.update(input, dt, &scene, t);
            }
            ship.position
        };
        let difference = (run(1.0 / 30.0) - run(1.0 / 144.0)).length();
        println!("orbita, diferencia entre 30 y 144 fps: {difference:.3} unidades");
        assert!(difference < 0.5);
    }

    #[test]
    fn volando_normal_el_arriba_sigue_siendo_y_exacto() {
        let mut ship = Ship::new(Vec3::ZERO);
        ship.yaw = 0.7;
        ship.pitch = 0.4;
        let f = ship.forward();
        let (r, u, forward) = ship.basis();
        assert_eq!(ship.camera_up().y, 1.0);
        assert_eq!((forward - f).length(), 0.0);
        assert_eq!((r - f.cross(Vec3::UP).normalized()).length(), 0.0);
        assert!(is_finite(u));
    }

    #[test]
    fn se_puede_viajar_entre_todos_los_mundos_sin_chocar() {
        let scene = Scene::new();
        let mut worst_margin = f32::MAX;
        let mut slowest = 0.0f32;
        let mut trips = 0;

        // de cada mundo a cada otro, en varios momentos de sus orbitas y con buenos y malos fps
        for from in 1..=5 {
            for to in 1..=5 {
                if from == to {
                    continue;
                }
                for start in [0.0, 40.0, 200.0, 777.0] {
                    for dt in [1.0 / 60.0, 1.0 / 20.0] {
                        let mut ship = Ship::new(scene.body_position(from, start) + Vec3::new(0.0, scene.bodies[from].radius + 3.0, 0.0));
                        ship.start_warp(to, &scene, start);
                        assert!(ship.warping());

                        let mut t = start;
                        while ship.warping() {
                            t += dt;
                            let before = ship.position;
                            ship.update(ShipInput::default(), dt, &scene, t);

                            // sin teletransporte: nunca avanza mas de lo q da la velocidad
                            assert!((ship.position - before).length() <= (WARP_SPEED + 10.0) * dt);

                            // y nunca le entra a ningun cuerpo
                            for (j, b) in scene.bodies.iter().enumerate() {
                                let margin = (scene.body_position(j, t) - ship.position).length() - collision_radius(b.radius);
                                worst_margin = worst_margin.min(margin);
                            }
                            assert!(t - start < WARP_MAX_TIME, "viaje {from} a {to} no termino");
                        }
                        slowest = slowest.max(t - start);
                        trips += 1;

                        // llego a su destino, en rango de la f y moviendose con el planeta
                        let radius = scene.bodies[to].radius;
                        let arrival = (scene.body_position(to, t) - ship.position).length();
                        assert!((arrival - WARP_ARRIVE_RADII * radius).abs() < 1.0, "viaje {from} a {to}: quedo a {arrival}");
                        assert!((ship.velocity - body_velocity(&scene, to, t)).length() < 0.5);
                    }
                }
            }
        }
        println!("{trips} viajes, el mas largo tardo {slowest:.1} s, lo mas cerca de una colision fue {worst_margin:.2}");
        assert!(worst_margin > 0.0);
    }

    #[test]
    fn llegar_a_un_planeta_con_luna_no_choca_con_la_luna() {
        let scene = Scene::new();
        let dt = 1.0 / 60.0;
        let gotham = scene.bodies.iter().position(|b| b.name == "Gotham").unwrap();
        let moon = scene.bodies.iter().position(|b| b.name == "Central City").unwrap();
        let moon_collision = collision_radius(scene.bodies[moon].radius);
        let mut worst_margin = f32::MAX;
        let mut cancelled = 0;

        // la luna da la vuelta cada 10 s, asi q pruebo salir en muchos momentos de su vuelta
        for k in 0..120 {
            let start = k as f32 * 0.37;
            let mut ship = Ship::new(scene.body_position(5, start) + Vec3::new(0.0, 8.0, 0.0));
            ship.start_warp(gotham, &scene, start);
            let mut t = start;
            while ship.warping() {
                t += dt;
                ship.update(ShipInput::default(), dt, &scene, t);
                let margin = (scene.body_position(moon, t) - ship.position).length() - moon_collision;
                worst_margin = worst_margin.min(margin);
            }
            let arrival = (scene.body_position(gotham, t) - ship.position).length();
            if (arrival - WARP_ARRIVE_RADII * scene.bodies[gotham].radius).abs() > 1.0 {
                cancelled += 1;
            }
        }
        println!("llegando a gotham: {cancelled} de 120 viajes cortados, lo mas cerca de la luna fue {worst_margin:.2}");
        assert_eq!(cancelled, 0);
        assert!(worst_margin >= -0.01);
    }

    #[test]
    fn el_sol_en_medio_se_rodea() {
        let scene = Scene::new();
        let dt = 1.0 / 60.0;
        // la nave esta del otro lado del sol, justo en linea con themyscira
        let target = scene.body_position(1, 0.0);
        let mut ship = Ship::new(target * -2.0);
        ship.start_warp(1, &scene, 0.0);

        let mut t = 0.0;
        let mut closest_to_sun = f32::MAX;
        while ship.warping() {
            t += dt;
            ship.update(ShipInput::default(), dt, &scene, t);
            closest_to_sun = closest_to_sun.min(ship.position.length());
        }
        println!("cruzando el sol: lo mas cerca fue {closest_to_sun:.2} del centro");
        assert!(closest_to_sun > collision_radius(scene.bodies[0].radius));
        assert!((scene.body_position(1, t) - ship.position).length() < WARP_BRAKE_RADII * scene.bodies[1].radius);
    }

    #[test]
    fn las_fases_van_en_orden_y_el_efecto_prende_a_medio_viaje() {
        let scene = Scene::new();
        let dt = 1.0 / 60.0;
        let mut ship = Ship::new(scene.body_position(5, 0.0) + Vec3::new(0.0, 8.0, 0.0));
        ship.start_warp(1, &scene, 0.0);

        let mut seen = Vec::new();
        let mut t = 0.0;
        let mut strongest = 0.0f32;
        while ship.warping() {
            let phase = ship.warp_trip.as_ref().unwrap().phase;
            if seen.last() != Some(&phase) {
                seen.push(phase);
            }
            // alineandose la nave todavia no prende nada
            if phase == WarpPhase::Align {
                assert_eq!(ship.warp_effect(), 0.0);
            }
            t += dt;
            ship.update(ShipInput::default(), dt, &scene, t);
            strongest = strongest.max(ship.warp_effect());
        }
        assert_eq!(seen, vec![WarpPhase::Align, WarpPhase::Accelerate, WarpPhase::Cruise, WarpPhase::Brake]);
        assert!(strongest > 0.9);

        // al terminar se va apagando solo hasta quedar en 0
        for _ in 0..300 {
            t += dt;
            ship.update(ShipInput::default(), dt, &scene, t);
        }
        assert_eq!(ship.warp_effect(), 0.0);
    }

    #[test]
    fn un_destino_cercano_no_arranca_el_viaje() {
        let scene = Scene::new();
        let mut ship = orbiting_ship(&scene, 1, Vec3::UP, 3.0);
        ship.start_warp(1, &scene, 0.0);
        assert!(!ship.warping());
        // y tampoco le solto la orbita
        assert_eq!(ship.orbit_body(), Some(1));
    }

    #[test]
    fn cancelar_deja_una_velocidad_controlable() {
        let scene = Scene::new();
        let dt = 1.0 / 60.0;
        let mut ship = Ship::new(scene.body_position(5, 0.0) + Vec3::new(0.0, 8.0, 0.0));
        ship.start_warp(1, &scene, 0.0);
        for step in 1..=150 {
            ship.update(ShipInput::default(), dt, &scene, step as f32 * dt);
        }
        assert!(ship.velocity.length() > WARP_CANCEL_SPEED);
        ship.cancel_warp();
        assert!(!ship.warping());
        assert!(ship.velocity.length() <= WARP_CANCEL_SPEED + 1e-3);
    }

    #[test]
    fn la_nariz_no_se_pasa_del_tope_y_responde_al_volver() {
        let dt = 1.0 / 60.0;
        let mut ship = Ship::new(Vec3::ZERO);
        for _ in 0..600 {
            ship.fly(ShipInput { climb: 1.0, ..Default::default() }, dt);
        }
        assert!(ship.pitch <= PITCH_LIMIT);

        // apenas apreto pa abajo la nariz tiene q empezar a bajar, sin quedarse pegada
        let top = ship.pitch;
        for _ in 0..5 {
            ship.fly(ShipInput { climb: -1.0, ..Default::default() }, dt);
        }
        assert!(ship.pitch < top);
    }
}
