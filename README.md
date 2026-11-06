# Proyecto Final – Sistema Solar con Software Renderer (Rust)

Sistema solar pequeño que se explora con una nave. Todo el 3D se dibuja con **nuestro propio pipeline por software**:
raylib solo se usa para abrir la ventana, leer el teclado, mostrar el framebuffer como textura 2D y escribir el texto del HUD.

## Integrantes y contribuciones

| Integrante | Parte | Archivos principales |
|---|---|---|
| Diego Guevara | Motor / pipeline de render, culling, LOD, órbitas, mapeo de normales | `renderer.rs`, `math.rs`, `mesh.rs`, `normal_map.rs` |
| _(integrante 2)_ | Planetas, shaders, superficie | `scene.rs`, `shaders.rs`, `noise.rs` |
| _(integrante 3)_ | Nave y navegación | `ship.rs`, `assets/Halcon_Milenario.obj` / `.mtl` / `.blend` |

## Cómo ejecutar

Requisitos:
- [Rust](https://rustup.rs) (stable).
- **CMake** y **LLVM/Clang**, que el crate `raylib` necesita para compilar raylib.
  - Windows: `winget install Kitware.CMake LLVM.LLVM` y las *Visual Studio Build Tools* (C++).
  - Linux: `sudo apt install cmake clang libx11-dev libxrandr-dev libxinerama-dev libxcursor-dev libxi-dev libgl1-mesa-dev`

```bash
cargo run --release
```

> Siempre usen `--release`: un renderer por software sin optimizaciones corre muy lento.

Para generar capturas sin abrir la ventana (útil para probar cambios):

```bash
cargo run --release -- --snapshot   # genera snap_*.ppm e imprime ms/cuadro y triángulos
```

## Controles

| Tecla | Acción |
|---|---|
| W / S | Acelerar / reversa |
| A / D | Girar izquierda / derecha |
| Flechas ↑ / ↓ | Subir / bajar la nariz |
| Shift | Turbo: empuja hacia adelante y sale la llama azul por detrás |
| Espacio | Frenar |
| C | Activar/desactivar frustum culling (para comparar contadores) |
| L | Activar/desactivar LOD (para comparar contadores) |
| O | Mostrar/ocultar las órbitas de los planetas |
| N | Activar/desactivar el mapeo de normales (para comparar) |
| P | Modo foto: congela la simulación |
| 1 a 5 | Elegir el mundo destino del warp (se ve en el HUD y con un anillo en el radar) |
| Tab | Viajar al destino con warp; otro Tab cancela el viaje |
| F | Modo órbita cerca de un planeta (el HUD avisa cuando se puede). W/S lo rodean, A/D cambian el rumbo, flechas cambian la altura. F o Shift lo sueltan |
| G | Activar/desactivar la gravedad de los planetas y el sol |
| E | Escanear el mundo cercano (el HUD avisa cuando se puede): abre su ficha por 6 s |
| H | Mostrar/ocultar ayuda |

## Cómo funciona el pipeline (`src/renderer.rs`)

1. **Culling**: la esfera envolvente de cada objeto se prueba contra el frustum de la cámara; si está fuera, el objeto no se procesa.
2. **Vértices**: `clip = Projection * View * Model * posición`.
3. **Recorte** contra el plano cercano (Sutherland–Hodgman), necesario para acercarse a los planetas sin errores.
4. **División de perspectiva**, viewport y **back-face culling**.
5. **Rasterizado** con coordenadas baricéntricas e interpolación corregida por perspectiva.
6. **Z-buffer** (se guarda `1/w`; más grande = más cerca) y G-buffer con los datos del fragmento ganador.
7. **Objetos aditivos** (`draw_mesh_additive`, la llama del turbo): se dibujan al final, hacen la prueba de profundidad pero no escriben
   en el z-buffer, y su shader se SUMA en un buffer de luz (`glow`). Así se ven como luz semitransparente.
8. **Fragment shaders** en paralelo (rayon), una vez por píxel visible (*sombreado diferido*). Los píxeles vacíos usan el shader del cielo.
   Al final se suma el buffer de luz de los objetos aditivos.
   La nebulosa del cielo (ruido caro y que no cambia) se precalcula una vez en una textura de 256x128;
   esto bajó el sombreado de ~4.5 a ~2.8 ms por cuadro.

`--snapshot` imprime el tiempo por cuadro separado en **geometría** (pasos 1–7) y **sombreado** (paso 8),
para saber qué etapa optimizar.

## Estructura

```
src/
  main.rs       ventana raylib, loop, HUD, modo --snapshot
  math.rs       Vec3, Vec4, Mat4 (traslación, rotación, escala, look_at, perspectiva)
  mesh.rs       Vertex/Mesh, carga de .obj (tobj), icosfera con subdivisión, normales
  renderer.rs   pipeline por software (ver arriba) + contadores
  shaders.rs    shaders: sol (animado), Themyscira (biomas: agua especular / tierra con normales), Krypton (Voronoi: facetas + venas emisivas), Gotham (manzanas, río, lagos, carreteras y parques; ventanas y tráfico de noche, animado), Atlantis (olas animadas en la normal + arrecifes), Oa (circuitos + anillos de energía + borde que respira), nave (Blinn-Phong), llama del turbo (aditiva), cielo de estrellas
  noise.rs      ruido de gradiente 3D + fBm con semilla + Voronoi 3D (`voronoi3`)
  normal_map.rs mapa de normales procedural (espacio tangente) + base TBN
  texture.rs    textura propia (u, v) con interpolación bilineal
  scene.rs      sol y planetas: órbitas, rotación, LODs, posición con jerarquía (lunas)
  props.rs      objetos sobre la superficie (por ahora solo las estructuras)
  ship.rs       nave (física simple, colisión), shader del Halcón y cámara de persecución
  interaction.rs estado del juego: cuerpo más cercano, escaneo, destino del warp, modo foto, cristales coleccionables
assets/
  Halcon_Milenario.obj / .mtl / .blend   nave actual: Halcón Milenario low-poly (1680 triángulos)
  nave.obj / nave.mtl / nave.blend   primera nave, modelada en Blender (respaldo si falla la carga del Halcón)
```

## Características del menú que solicitamos evaluar

Estado: ✅ hecho · 🟡 parcial · ⬜ pendiente

| Característica | Pts | Estado | Dónde |
|---|---|---|---|
| Requisito: pipeline propio, modelo/vista/proyección, z-buffer | – | ✅ | `renderer.rs`, `math.rs` |
| Requisito: sol + nave propia + planeta que orbita y rota (con `dt`) | – | ✅ | `scene.rs`, `ship.rs` |
| Requisito: fragment shader propio | – | ✅ | `shaders.rs` |
| Planetas distinguibles (10 c/u, máx 5) | 50 | ✅ 5 de 5: Themyscira, Krypton, Gotham, Atlantis, Oa | `scene.rs` |
| Luna Central City: jerarquía de transformaciones (rúbrica: lunas) | – | ✅ orbita a Gotham (radio 9) mientras Gotham orbita al sol; rayos animados; se escanea con E | `Scene::body_position`, `CentralCityShader` |
| Nave modelada por nosotros que sigue a la cámara (rúbrica) | – | ✅ Halcón | `assets/Halcon_Milenario.obj`, `Ship`, `ChaseCamera` |
| Shaders con efecto distinguible (6 c/u, máx 4) | 24 | ✅ sol, Themyscira, Krypton, Gotham, Atlantis, Oa, nave (paneles y motores), llama del turbo, órbita, cielo | `shaders.rs`, `HalconShader` en `ship.rs` |
| Shader animado | 8 | ✅ sol (plasma), Gotham (ventanas que parpadean y tráfico), Atlantis (olas), Oa (anillos y respiración), Central City (rayos), llama del turbo | `SunShader`, `GothamShader`, `AtlantisShader`, `OaShader`, `CentralCityShader`, `ExhaustShader` |
| Deformación de geometría con ruido | 10 | ✅ Themyscira: islas con agua plana `1 + max(0, elev)·0.06`; Krypton: crestas ridged `1 - |fbm|`; Gotham: escalones por manzana | `scene.rs`, `themyscira_elevation` |
| Subdivisión procedural de geometría antes de rasterizar | 10 | ✅ icosfera | `Mesh::icosphere` |
| Descarte de objetos fuera de la vista + contador | 7 | ✅ tecla C; en el cinturón: 886 de 1508 objetos dibujados con C, todos sin C | `Renderer::sphere_visible` |
| LOD según distancia + contador | 8 | ✅ tecla L; en el cinturón: 22 800 triángulos con L contra 385 680 sin L (`--snapshot` imprime la comparación) | `Renderer::draw_lods` |
| Cinturón de asteroides (1500 rocas con semilla fija `ASTEROID_SEED`, 4 formas × 3 LODs, órbita de Kepler) | – | ✅ entre Krypton y Gotham | `exotics.rs` |
| Cometa en órbita elíptica (ecuación de Kepler con Newton) con cola que siempre apunta lejos del sol | – | ✅ también sale en el radar; la cola crece cerca del sol | `exotics.rs` (`Comet`) |
| Agujero negro lejano: lente gravitacional `β = θ − θE²/θ`, sombra, anillo de fotones y disco de acreción animado | – | ✅ en el cielo, arriba a la derecha del sol al empezar | `black_hole` en `shaders.rs` (`BH_*`) |
| Superman: vuela en loops alrededor de Krypton (1.6 radios, sube y baja) con estela que se desvanece | – | ✅ modelo `assets/heroes/superman.obj` (1444 triángulos + 2 LODs) | `creatures.rs`, `Renderer::draw_line_strip` |
| Flash: corre por el ecuador de Central City (una vuelta cada ~2 s) con estela amarilla | – | ✅ modelo `flash.obj` (1432 triángulos + 2 LODs) | `creatures.rs` (`flash_path`) |
| Batman: quieto en la azotea más alta de Gotham (el vértice más alto de su malla), gira con el planeta | – | ✅ modelo `batman.obj` (1624 triángulos + 2 LODs); falta la batiseñal de Sando (S8) | `creatures.rs` (`highest_point`, `standing_on`) |
| Aquaman montado en una ballena espacial que nada lento alrededor de Atlantis (jerarquía: Aquaman = ballena · traslación) | – | ✅ modelos `ballena.obj` (1342) y `aquaman.obj` (1366) + 2 LODs cada uno | `creatures.rs` (`whale_path`, `aquaman_path`) |
| Linterna Verde: flota sobre Oa con brillo verde emisivo y un aura aditiva que pulsan con el tiempo | – | ✅ modelo `linterna_verde.obj` (1396 triángulos + 2 LODs) | `creatures.rs` (`lantern_path`, `LanternShader`, `draw_creature_glows`) |
| Mujer Maravilla: estatua gigante de piedra blanca en la cima de Themyscira | – | ✅ modelo `mujer_maravilla.obj` (1546 triángulos + 2 LODs); falta el templo de Sando (S7) en ese punto | `creatures.rs` (`StoneShader`) |
| Colisión básica con planetas | 4 | ✅ | `Ship::update` |
| Aceleración/frenado/giro progresivos | 8 | ✅ velocidad de giro con inercia, igual a 30 y 144 FPS (`cargo test`) | `Ship::fly`, `next_spin_rate` |
| Modo de control local para rodear un planeta | 8 | ✅ tecla F, se ve en el HUD | `Ship::orbit_step`, `interaction::update` |
| Atracción gravitatoria | 10 | ✅ tecla G, se ve en el HUD | `Ship::gravity_accel` |
| Modelos sobre la superficie (5 c/u) + distribución procedural + orientación | 35 | ⬜ | |
| Mapeo de normales | 8 | ✅ tecla N, solo en la tierra de Themyscira | `normal_map.rs`, `ThemysciraShader` |
| Cristales coleccionables (extra) | – | ✅ 2 por mundo con semilla fija, contador en el HUD | `interaction.rs`, `COLLECTIBLE_SEED`, `CrystalShader` |
| Órbitas visibles (rúbrica) | – | ✅ tecla O | `Renderer::draw_line_loop`, `OrbitShader` |
| Warping animado (rúbrica) | – | ✅ Tab viaja por el espacio sin teletransporte y esquiva lo que tenga en medio | `Ship::warp_step`, `warp_streaks` en `shaders.rs`, FOV en `main.rs` |
| HUD: ficha del mundo escaneado, destino y radar (claridad del recorrido) | – | ✅ tecla E y 1–5; radar abajo a la derecha dibujado en nuestro framebuffer | `hud.rs`, `Game::draw_overlay` |

## Uso de IA y recursos externos

Se usó asistencia de IA (Claude) para la estructura inicial del motor. Todo el código está comentado y el equipo lo revisó y entiende.
Los modelos de los personajes de la Liga (`assets/heroes/`) se construyen en Blender con el script
`assets/heroes/modelar_heroes.py` (escrito con ayuda de IA): arma cada personaje con primitivas low-poly (cuerpo con elipsoides y extremidades cónicas, capas de tela con Solidify, pose por articulaciones),
lo exporta a `.obj` con dos LODs (Decimate) y guarda su `.blend`. Para regenerarlos:
`blender --background --python assets/heroes/modelar_heroes.py`.
Librerías: `raylib` (ventana/entrada/textura), `tobj` (lectura de .obj), `rayon` (paralelismo).
