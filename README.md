# Sistema Solar — Renderer por software

Proyecto final de Gráficas por Computadora (2026). Un pequeño sistema solar con los mundos de la
Liga de la Justicia que se explora en nave, dibujado con **nuestro propio pipeline de render por
software**: matrices, recorte, rasterizado, z-buffer y shaders están escritos en Rust.
raylib solo se usa para abrir la ventana, leer el teclado, mostrar el framebuffer como una imagen
2D, escribir el texto del HUD y reproducir los sonidos.

## Integrantes y contribuciones

| Integrante | Contribución |
|------------|--------------|
| Diego Sandoval | Modelo del Halcón Milenario en Blender; planetas Themyscira, Krypton, Gotham, Atlantis y Oa (superficies y shaders); objetos de superficie (árboles, cristales, edificios, corales) y su distribución procedural; pantalla completa; sonido; aviso de calor y explosión al chocar con el Sol; tour automático y pantalla de inicio |
| Diego Guevara | Base del motor (pipeline de render, sol, planeta y nave); órbitas visibles; mapeo de normales; cielo con nebulosa y agujero negro; efecto visual del warp; HUD con ficha y radar; luna Central City; cinturón de asteroides y cometa; personajes de la Liga; monumentos y detalles de cada mundo; optimizaciones del renderer |
| Adrián López | Giro progresivo; gravedad de los planetas; modo órbita; viaje warp; estado del juego y teclas (escaneo, destino, modo foto); cristales coleccionables; niveles de detalle (`draw_lods`); paneles y motores del shader del Halcón |

## Cómo ejecutarlo

Requisitos:

- [Rust](https://www.rust-lang.org/tools/install) estable (`cargo`).
- CMake y un compilador de C: la primera compilación construye raylib desde su código.
  En Windows sirven las *Build Tools* de Visual Studio.

```bash
cargo run --release
```

Usen siempre `--release`: el renderer por software es lento sin optimizaciones.

Otros modos:

```bash
cargo run --release -- --snapshot
```

Sin abrir la ventana, guarda capturas `snap_*.ppm` de cada mundo, monumento, personaje, la
explosión y la pantalla de inicio, e imprime los milisegundos por cuadro. Al final compara el
cinturón de asteroides con culling y LOD prendidos y apagados.

```bash
cargo test --release
```

Corre las pruebas: órbitas, colisiones, gravedad, warp, tour, explosión, sonidos, etc.

## Controles

| Tecla | Acción |
|-------|--------|
| Enter | Empezar (en la pantalla de inicio) |
| T | Tour automático: hace solo el recorrido completo (también desde la pantalla de inicio) |
| W / S | Acelerar / frenar |
| A / D | Girar |
| Flechas arriba / abajo | Subir / bajar la nariz |
| Shift | Turbo (con la órbita enganchada, la suelta) |
| Espacio | Freno fuerte |
| F | Engancharse a la órbita del mundo cercano / soltarla |
| E | Escanear el mundo cercano (muestra su ficha) |
| 1 – 5 | Elegir destino: Themyscira, Krypton, Gotham, Atlantis, Oa |
| Tab | Warp al destino (otro Tab lo cancela) |
| G | Prender / apagar la gravedad |
| P | Modo foto (congela todo; la cámara sigue) |
| C | Prender / apagar el culling por frustum |
| L | Prender / apagar los niveles de detalle (LOD) |
| O | Mostrar / ocultar las órbitas |
| N | Prender / apagar el mapeo de normales |
| M | Silenciar el sonido |
| H | Mostrar / ocultar la ayuda |
| F11 | Pantalla completa / ventana |

Cualquier tecla de vuelo detiene el tour y devuelve el control.

## Recorrido sugerido para la demo

1. En la pantalla de inicio, **T** arranca el tour automático: vuelo espacial, warp a un mundo,
   escaneo, vuelta completa pegado a la superficie y alejamiento, mundo tras mundo.
2. Para pilotear: **1–5** para elegir un mundo, **Tab** para viajar, **F** al llegar para rodearlo,
   **E** para ver su ficha y **F** o **Shift** para alejarse.
3. Con **C** y **L** se ve en el HUD cómo cambian los objetos y triángulos procesados.
4. Si la nave se acerca demasiado al Sol aparece el aviso de calor; si lo toca, explota y
   reaparece junto al mundo elegido.

## Características del menú que solicitamos evaluar

### Planetas

- **5 planetas distinguibles que orbitan y rotan:** Themyscira (islas y mar), Krypton (cristales y
  venas rojas), Gotham (planeta-ciudad nocturno), Atlantis (océano con olas) y Oa (circuitos
  verdes). Además, el **Sol** y la **luna Central City**, que orbita a Gotham (jerarquía de
  transformaciones). Se configuran en `src/scene.rs`; el movimiento depende del tiempo.

### Superficie

- **Tipos de modelo propios sobre los planetas** (Blender, en `assets/props/`): cristales
  (Krypton), árboles (Themyscira), edificios (Gotham) y corales (Atlantis), repartidos por todo el
  planeta. También hay monumentos: templo, Fortaleza de la Soledad, Torre Wayne, ciudad de
  Atlantis, S.T.A.R. Labs, torres y batería de Oa (carpeta `src/mundos/` y `src/oa.rs`).
- **Distribución procedural reproducible:** `scatter` en `src/props.rs` usa hash y ruido con la
  semilla `PROP_SEED`; con la misma semilla salen las mismas posiciones. Cada mundo filtra
  dónde pueden ir (por ejemplo, árboles solo en el pasto).
- **Posición y orientación sobre la superficie curva mientras el planeta rota:** `instance_model`
  en `src/props.rs` deja cada objeto parado sobre el terreno ("arriba" = la normal del planeta)
  usando el marco del planeta (`Scene::body_frame`).

### Renderizado

- **Shaders propios distinguibles** (`src/shaders.rs`): `SunShader`, `ThemysciraShader`,
  `KryptonShader`, `GothamShader`, `AtlantisShader`, `OaShader`, `CentralCityShader`, el cielo
  (`Sky`: estrellas, nebulosa y agujero negro) y el de la nave (`HalconShader`, en `src/ship.rs`).
- **Shaders animados:** las olas de Atlantis, los anillos que pulsan en Oa, los rayos de Central
  City, las manchas del Sol y el tráfico de Gotham cambian con el tiempo.
- **Mapeo de normales:** `src/normal_map.rs`, aplicado a Themyscira (tecla **N** para comparar).
- **Deformación de la geometría con ruido:** islas de Themyscira, crestas de Krypton, manzanas de
  Gotham, Oa y los cráteres de Central City (funciones `*_surface` en `src/scene.rs`).
- **Generación / subdivisión procedural de geometría antes de rasterizar:** las esferas son
  icosferas subdivididas (`Mesh::icosphere` en `src/mesh.rs`). También se generan en el pipeline
  la llama del turbo, anillos, cúpulas, conos de luz y los pedazos de la nave al explotar
  (`src/explosion.rs`).

### Navegación

- **Modo de control local para rodear un planeta:** tecla **F** (modo órbita en `src/ship.rs`):
  W/S avanzan alrededor del planeta, A/D cambian el rumbo y las flechas, la altura.
- **Aceleración, frenado y giro progresivos e independientes de los FPS:** todo se multiplica por
  `dt` y los frenos usan exponenciales (hay pruebas que comparan 30 y 144 FPS).
- **Atracción gravitatoria:** cada cuerpo jala la nave con `a = gravedad / d²` (tecla **G**).
- **Colisión básica:** la nave no puede atravesar ningún cuerpo (`Ship::collide`). Tocar el Sol
  destruye la nave.

### Rendimiento

- **Descarte fuera de la vista (frustum culling):** esfera envolvente contra el volumen de visión
  en `src/renderer.rs` (tecla **C**).
- **Niveles de detalle (LOD) según la distancia:** planetas, objetos y personajes cambian de malla
  según su tamaño en pantalla (tecla **L**).
- **Comparación:** el HUD muestra objetos dibujados/descartados y triángulos que entran y se
  rasterizan; `--snapshot` imprime la comparación con todo prendido y apagado.

### Otros puntos de la rúbrica

- **Warp animado:** tecla **Tab**. La nave se alinea, acelera, viaja esquivando los cuerpos y
  frena al llegar; el campo de visión se abre y el cielo se estira en rayas.
- **Nave modelada que sigue a la cámara:** `assets/Halcon_Milenario.obj` con su shader propio y
  cámara de persecución (`ChaseCamera`).
- **Skybox con estrellas:** cielo procedural con estrellas, nebulosa y un agujero negro lejano.
- **Órbitas de los planetas:** líneas punteadas animadas (tecla **O**).

### Extras

- Personajes de la Liga en sus mundos (`src/creatures.rs`), cinturón de asteroides y cometa con
  órbita elíptica (`src/exotics.rs`), atmósferas, nubes, llamaradas solares y eclipse de Central
  City sobre Gotham.
- Cristales coleccionables alrededor de cada mundo, radar, ficha de escaneo y modo foto.
- Sonido: motor, warp, efectos y un ambiente por mundo (`src/audio.rs`). Los `.wav` se generan con
  `assets/sonidos/generar_sonidos.py` (Python + numpy).
- Aviso de calor y explosión al chocar con el Sol (`src/explosion.rs`).
- Pantalla de inicio y tour automático (`src/tour.rs`).

## Estructura del código

| Archivo | Qué hace |
|---------|----------|
| `src/main.rs` | Ventana, ciclo del juego, HUD, pantalla de inicio y modo `--snapshot` |
| `src/renderer.rs` | Pipeline de render por software |
| `src/math.rs`, `src/mesh.rs` | Vectores y matrices, mallas, carga de `.obj` e icosferas |
| `src/scene.rs` | Sol, planetas, luna, órbitas y objetos de superficie |
| `src/shaders.rs` | Fragment shaders de los planetas, objetos, órbitas y cielo |
| `src/ship.rs` | Nave, vuelo, órbita, warp, gravedad, colisión y cámara |
| `src/interaction.rs` | Escaneo, destino, órbita, cristales y modo foto |
| `src/props.rs` | Distribución y colocación de objetos sobre la superficie |
| `src/mundos/`, `src/oa.rs` | Monumentos y detalles de cada mundo |
| `src/creatures.rs` | Personajes de la Liga |
| `src/exotics.rs` | Cinturón de asteroides y cometa |
| `src/explosion.rs` | Explosión de la nave y reaparición |
| `src/tour.rs` | Tour automático |
| `src/audio.rs` | Sonido |
| `src/hud.rs` | Radar, ficha y tintes de pantalla dibujados en el framebuffer |
| `src/noise.rs`, `src/normal_map.rs`, `src/texture.rs` | Ruido procedural, mapa de normales y texturas |

## Recursos externos y asistencia

- [raylib](https://www.raylib.com/) (crate `raylib` 5.5): ventana, teclado, mostrar el framebuffer,
  texto del HUD y reproducir sonido. No se usan sus funciones de dibujo 3D, modelos, cámara,
  materiales ni shaders.
- [tobj](https://crates.io/crates/tobj): lectura de archivos `.obj`.
- [rayon](https://crates.io/crates/rayon): paralelismo en el sombreado y el rasterizado.
- [Blender](https://www.blender.org/): modelado de la nave, los personajes y los objetos de
  superficie (los personajes y varios objetos se generan con los scripts `modelar_*.py` de
  `assets/`).
- Python con numpy: generación de los efectos de sonido (`assets/sonidos/generar_sonidos.py`).

Se usó asistencia de IA como apoyo durante el desarrollo. El equipo entiende y puede explicar
todo el código entregado.
