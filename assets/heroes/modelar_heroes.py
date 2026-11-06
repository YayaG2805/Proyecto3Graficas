# MODELOS LOW-POLY DE LA LIGA para el Proyecto 3 (Diego).
#
# Se ejecuta con Blender (no necesita abrir la ventana):
#     blender --background --python assets/heroes/modelar_heroes.py
#
# Cada personaje se arma con primitivas de Blender (cajas, cilindros, esferas y conos),
# se une en un solo objeto con sombreado plano y se exporta a assets/heroes/<nombre>.obj
# (triangulado y con normales), junto con su .blend y dos LODs hechos con Decimate:
# <nombre>_lod1.obj (50 %) y <nombre>_lod2.obj (25 %).
#
# Ejes: en Blender el personaje mira hacia +Y y "arriba" es +Z. El exportador OBJ convierte
# (x, y, z) de Blender en (x, z, -y), así que en el archivo mira hacia -Z: lo que espera el juego.
#
# COLORES con una "textura de paleta": cada parte lleva una coordenada u distinta
# (u = (banda + 0.5) / bandas) y el HeroShader del juego elige el color de la paleta según u.
# Así la capa sale roja y el traje azul aunque el juego no lea materiales.

import math
import os

import bpy
from mathutils import Vector

OUT = os.path.dirname(os.path.abspath(__file__))
LOG = os.environ.get("HEROES_LOG")


def log(text):
    print(text)
    if LOG:
        with open(LOG, "a", encoding="utf-8") as f:
            f.write(text + "\n")


def rad(angles):
    return [math.radians(a) for a in angles]


class Model:
    """Un personaje en construcción: se le agregan partes y al final se exporta."""

    def __init__(self, name, bands):
        bpy.ops.wm.read_factory_settings(use_empty=True)
        self.name = name
        self.bands = bands
        self.parts = []

    def _part(self, band):
        # Pinta TODA la parte con la misma u: así el shader sabe de qué color es.
        obj = bpy.context.active_object
        mesh = obj.data
        if not mesh.uv_layers:
            mesh.uv_layers.new(name="UVMap")
        u = (band + 0.5) / self.bands
        for loop_uv in mesh.uv_layers.active.data:
            loop_uv.uv = (u, 0.5)
        self.parts.append(obj)
        return obj

    def box(self, band, loc, size, rot=(0, 0, 0)):
        bpy.ops.mesh.primitive_cube_add(size=1, location=loc, rotation=rad(rot))
        bpy.context.active_object.scale = size
        return self._part(band)

    def ball(self, band, loc, radius, scale=(1, 1, 1), segments=8, rings=6):
        bpy.ops.mesh.primitive_uv_sphere_add(segments=segments, ring_count=rings, radius=radius, location=loc)
        bpy.context.active_object.scale = scale
        return self._part(band)

    def disc(self, band, loc, radius, depth, rot=(0, 0, 0), vertices=12):
        bpy.ops.mesh.primitive_cylinder_add(vertices=vertices, radius=radius, depth=depth, location=loc, rotation=rad(rot))
        return self._part(band)

    def cone(self, band, loc, r1, r2, depth, rot=(0, 0, 0), vertices=8):
        bpy.ops.mesh.primitive_cone_add(vertices=vertices, radius1=r1, radius2=r2, depth=depth, location=loc, rotation=rad(rot))
        return self._part(band)

    def limb(self, band, a, b, radius, vertices=8):
        """Cilindro que va del punto a al punto b (brazos, piernas, mango del tridente...)."""
        a, b = Vector(a), Vector(b)
        d = b - a
        rot = d.to_track_quat("Z", "Y").to_euler()
        bpy.ops.mesh.primitive_cylinder_add(vertices=vertices, radius=radius, depth=d.length, location=(a + b) / 2, rotation=rot)
        return self._part(band)

    def _export(self, filename):
        bpy.ops.wm.obj_export(
            filepath=os.path.join(OUT, filename + ".obj"),
            export_selected_objects=True,
            export_triangulated_mesh=True,
            export_normals=True,
            export_uv=True,
            export_materials=False,
            forward_axis="NEGATIVE_Z",
            up_axis="Y",
        )

    def save(self):
        # Unir todas las partes en un solo objeto y dejar las transformaciones aplicadas.
        bpy.ops.object.select_all(action="DESELECT")
        for p in self.parts:
            p.select_set(True)
        bpy.context.view_layer.objects.active = self.parts[0]
        bpy.ops.object.join()
        obj = bpy.context.active_object
        obj.name = self.name
        bpy.ops.object.transform_apply(location=True, rotation=True, scale=True)
        bpy.ops.object.shade_flat()

        tris = sum(len(p.vertices) - 2 for p in obj.data.polygons)
        bpy.ops.wm.save_as_mainfile(filepath=os.path.join(OUT, self.name + ".blend"))
        self._export(self.name)
        counts = [tris]
        # LODs: Decimate reduce los triángulos a la mitad cada vez.
        for k in (1, 2):
            mod = obj.modifiers.new("Decimate", "DECIMATE")
            mod.ratio = 0.5
            bpy.ops.object.modifier_apply(modifier=mod.name)
            counts.append(sum(len(p.vertices) - 2 for p in obj.data.polygons))
            self._export(f"{self.name}_lod{k}")
        log(f"{self.name}: triangulos LOD0/1/2 = {counts}")


def superman():
    # Bandas: 0 azul, 1 rojo, 2 piel, 3 amarillo, 4 negro. Volando acostado, puño al frente.
    m = Model("superman", 5)
    m.box(0, (0, 0, 0), (0.5, 0.75, 0.3))  # torso
    m.box(3, (0, 0.12, -0.155), (0.2, 0.2, 0.02), rot=(0, 0, 45))  # escudo amarillo (en el pecho, abajo)
    m.box(1, (0, 0.12, -0.165), (0.12, 0.12, 0.02), rot=(0, 0, 45))  # "S" roja
    m.box(3, (0, -0.3, 0), (0.52, 0.06, 0.32))  # cinturón
    m.box(1, (0, -0.4, 0), (0.5, 0.14, 0.31))  # calzón rojo
    m.limb(2, (0, 0.37, 0.03), (0, 0.47, 0.06), 0.07)  # cuello
    m.ball(2, (0, 0.55, 0.08), 0.16)  # cabeza
    m.ball(4, (0, 0.52, 0.17), 0.15, scale=(1, 1, 0.6))  # pelo
    m.limb(0, (0.2, 0.3, 0.02), (0.2, 0.85, 0.06), 0.07)  # brazo estirado al frente
    m.box(2, (0.2, 0.92, 0.06), (0.13, 0.13, 0.13))  # puño
    m.limb(0, (-0.3, 0.28, 0), (-0.32, -0.25, -0.02), 0.07)  # otro brazo pegado al cuerpo
    m.ball(2, (-0.32, -0.3, -0.02), 0.065)
    for x in (0.12, -0.12):
        m.limb(0, (x, -0.45, 0), (x * 1.08, -1.05, 0.04), 0.09)  # pierna
        m.limb(1, (x * 1.08, -1.0, 0.04), (x * 1.08, -1.3, 0.06), 0.095)  # bota roja
    # Capa: tres tramos sobre la espalda (+Z) que ondean hacia atrás.
    m.box(1, (0, -0.1, 0.18), (0.6, 0.75, 0.03), rot=(-4, 0, 0))
    m.box(1, (0, -0.75, 0.24), (0.66, 0.6, 0.03), rot=(-8, 0, 0))
    m.box(1, (0, -1.25, 0.32), (0.7, 0.45, 0.03), rot=(-14, 0, 0))
    m.save()


def flash():
    # Bandas: 0 rojo, 1 amarillo, 2 piel. Corriendo, inclinado hacia adelante.
    m = Model("flash", 3)
    m.box(0, (0, 0.05, 1.15), (0.42, 0.26, 0.55), rot=(-12, 0, 0))  # torso
    m.box(0, (0, 0, 0.8), (0.4, 0.26, 0.2))  # cadera
    m.box(1, (0, 0.02, 0.9), (0.43, 0.28, 0.05), rot=(-12, 0, 0))  # cinturón
    m.disc(1, (0, 0.215, 1.25), 0.09, 0.03, rot=(90, 0, 0))  # rayo del pecho
    m.ball(0, (0, 0.18, 1.6), 0.16)  # capucha
    m.box(2, (0, 0.31, 1.56), (0.16, 0.05, 0.12))  # cara
    for x in (0.17, -0.17):
        m.box(1, (x, 0.17, 1.66), (0.03, 0.12, 0.06))  # alitas de las orejas
    # Pierna de adelante y pierna de atrás (zancada).
    m.limb(0, (0.11, 0, 0.78), (0.11, 0.35, 0.48), 0.085)
    m.limb(0, (0.11, 0.35, 0.48), (0.11, 0.25, 0.08), 0.075)
    m.box(1, (0.11, 0.3, 0.05), (0.13, 0.26, 0.1))
    m.limb(0, (-0.11, 0, 0.78), (-0.11, -0.25, 0.42), 0.085)
    m.limb(0, (-0.11, -0.25, 0.42), (-0.11, -0.6, 0.55), 0.075)
    m.box(1, (-0.11, -0.68, 0.58), (0.13, 0.24, 0.1), rot=(-20, 0, 0))
    # Brazos que se balancean al revés que las piernas.
    m.limb(0, (0.28, 0.05, 1.35), (0.32, -0.3, 1.05), 0.065)
    m.limb(0, (0.32, -0.3, 1.05), (0.32, -0.25, 0.8), 0.06)
    m.limb(0, (-0.28, 0.1, 1.35), (-0.3, 0.4, 1.1), 0.065)
    m.limb(0, (-0.3, 0.4, 1.1), (-0.3, 0.6, 1.3), 0.06)
    m.save()


def batman():
    # Bandas: 0 gris, 1 negro, 2 amarillo, 3 piel. De pie con la capa cayendo.
    m = Model("batman", 4)
    for x in (0.12, -0.12):
        m.limb(0, (x, 0, 0.85), (x, 0, 0.12), 0.09)  # pierna
        m.box(1, (x, 0.03, 0.08), (0.17, 0.26, 0.16))  # bota
        m.limb(0, (x * 2.6, 0, 1.48), (x * 2.75, 0.02, 0.95), 0.075)  # brazo
        m.limb(1, (x * 2.75, 0.02, 0.95), (x * 2.75, 0.03, 0.75), 0.07)  # guante
        m.cone(1, (x * 0.67, 0, 1.88), 0.045, 0, 0.16)  # oreja de la máscara
    m.box(1, (0, 0, 0.85), (0.42, 0.27, 0.16))  # calzón
    m.box(2, (0, 0, 0.95), (0.45, 0.29, 0.06))  # cinturón
    m.box(0, (0, 0, 1.25), (0.5, 0.28, 0.58))  # torso
    m.disc(2, (0, 0.145, 1.3), 0.11, 0.02, rot=(90, 0, 0))  # óvalo amarillo
    m.box(1, (0, 0.158, 1.3), (0.16, 0.01, 0.05))  # murciélago
    m.ball(1, (0, 0, 1.7), 0.16)  # máscara
    m.box(3, (0, 0.1, 1.62), (0.15, 0.08, 0.09))  # quijada
    m.box(1, (0, -0.05, 1.5), (0.62, 0.3, 0.1))  # hombros de la capa
    m.box(1, (0, -0.18, 1.2), (0.7, 0.04, 0.6), rot=(-4, 0, 0))  # capa arriba
    m.box(1, (0, -0.24, 0.62), (0.85, 0.04, 0.6), rot=(-10, 0, 0))  # capa abajo
    m.save()


def aquaman():
    # Bandas: 0 naranja, 1 verde, 2 piel, 3 rubio, 4 dorado. De pie con el tridente.
    m = Model("aquaman", 5)
    for x in (0.12, -0.12):
        m.limb(1, (x, 0, 0.85), (x, 0, 0.1), 0.09)
        m.box(1, (x, 0.03, 0.06), (0.16, 0.24, 0.12))
    m.box(1, (0, 0, 0.88), (0.42, 0.27, 0.2))  # cadera
    m.box(4, (0, 0, 0.98), (0.44, 0.29, 0.06))  # cinturón dorado
    m.box(0, (0, 0, 1.25), (0.48, 0.28, 0.55))  # camisa naranja
    m.limb(1, (0.3, 0, 1.48), (0.36, 0.12, 1.05), 0.07)
    m.ball(2, (0.37, 0.14, 1.0), 0.06)
    m.limb(1, (-0.3, 0, 1.48), (-0.34, 0, 0.95), 0.07)
    m.ball(2, (-0.34, 0, 0.9), 0.06)
    m.ball(2, (0, 0, 1.7), 0.15)  # cabeza
    m.ball(3, (0, -0.04, 1.75), 0.16, scale=(1.05, 1, 0.9))  # pelo
    m.box(3, (0, 0.11, 1.6), (0.17, 0.06, 0.08))  # barba
    # Tridente dorado en la mano derecha.
    m.limb(4, (0.37, 0.14, 0.1), (0.37, 0.14, 2.1), 0.025, vertices=6)
    m.box(4, (0.37, 0.14, 2.1), (0.3, 0.04, 0.04))
    for x in (0.25, 0.37, 0.49):
        m.cone(4, (x, 0.14, 2.27), 0.03, 0, 0.3, vertices=6)
    m.save()


def ballena():
    # Bandas: 0 azul oscuro, 1 vientre claro, 2 negro, 3 manchas cian. Nada hacia +Y.
    m = Model("ballena", 4)
    m.ball(0, (0, 0, 0), 1.0, scale=(0.55, 1.4, 0.5), segments=12, rings=8)  # cuerpo
    m.ball(1, (0, 0.1, -0.12), 1.0, scale=(0.5, 1.25, 0.4), segments=12, rings=8)  # vientre
    m.cone(0, (0, -1.55, 0.03), 0.22, 0.08, 0.6, rot=(90, 0, 0))  # base de la cola
    for s in (1, -1):
        m.box(0, (0.28 * s, -1.9, 0.06), (0.55, 0.25, 0.05), rot=(0, 0, 15 * s))  # aleta de la cola
        m.box(0, (0.55 * s, 0.35, -0.25), (0.55, 0.22, 0.04), rot=(0, 25 * s, -20 * s))  # aleta lateral
        m.ball(2, (0.4 * s, 0.95, 0.02), 0.06, segments=6, rings=4)  # ojo
    for y in (-0.6, -0.2, 0.2, 0.6):
        m.ball(3, (0, y, 0.47), 0.06, segments=6, rings=4)  # manchas que brillan en el lomo
    m.save()


def linterna_verde():
    # Bandas: 0 verde, 1 negro, 2 piel, 3 blanco. Flotando con el brazo del anillo al frente.
    m = Model("linterna_verde", 4)
    for x in (0.12, -0.12):
        m.limb(0, (x, 0, 0.85), (x, 0.05, 0.45), 0.09)
        m.limb(1, (x, 0.05, 0.45), (x, -0.05, 0.1), 0.08)
    m.box(1, (0, 0, 0.88), (0.42, 0.27, 0.2))  # cadera
    m.box(0, (0, 0, 1.25), (0.48, 0.28, 0.55))  # torso
    m.box(1, (0, 0, 1.47), (0.5, 0.29, 0.12))  # hombros
    m.disc(3, (0, 0.145, 1.25), 0.11, 0.02, rot=(90, 0, 0))  # emblema blanco
    m.disc(0, (0, 0.157, 1.25), 0.07, 0.02, rot=(90, 0, 0))  # linterna verde del emblema
    m.limb(0, (0.3, 0, 1.45), (0.32, 0.6, 1.42), 0.07)  # brazo al frente
    m.ball(3, (0.32, 0.66, 1.42), 0.075)  # guante blanco
    m.disc(0, (0.32, 0.72, 1.42), 0.05, 0.04, rot=(90, 0, 0))  # anillo
    m.limb(0, (-0.3, 0, 1.45), (-0.34, 0, 0.95), 0.07)
    m.ball(3, (-0.34, 0, 0.9), 0.065)
    m.ball(2, (0, 0, 1.7), 0.15)  # cabeza
    m.ball(1, (0, -0.03, 1.76), 0.15, scale=(1, 1, 0.7))  # pelo
    m.box(0, (0, 0.12, 1.72), (0.2, 0.04, 0.06))  # antifaz
    for x in (0.05, -0.05):
        m.box(3, (x, 0.145, 1.72), (0.04, 0.02, 0.025))  # ojos blancos
    m.save()


def mujer_maravilla():
    # Estatua de piedra (un solo color): espada en alto, escudo y pedestal.
    m = Model("mujer_maravilla", 1)
    m.box(0, (0, 0, 0.15), (0.9, 0.9, 0.3))  # pedestal
    m.box(0, (0, 0, 0.35), (0.7, 0.7, 0.1))
    for x in (0.11, -0.11):
        m.limb(0, (x, 0, 1.25), (x * 1.2, 0, 0.42), 0.085)  # pierna
    m.cone(0, (0, 0, 1.15), 0.3, 0.22, 0.28)  # falda
    m.box(0, (0, 0, 1.5), (0.42, 0.25, 0.5))  # torso
    m.ball(0, (0, 0, 1.95), 0.15)  # cabeza
    m.ball(0, (0, -0.05, 1.88), 0.16, scale=(1, 0.8, 1.3))  # pelo largo
    m.box(0, (0, 0.12, 2.03), (0.14, 0.03, 0.04))  # tiara
    m.limb(0, (0.27, 0, 1.7), (0.42, 0.05, 2.2), 0.065)  # brazo en alto
    m.box(0, (0.44, 0.05, 2.6), (0.06, 0.02, 0.75))  # hoja de la espada
    m.box(0, (0.44, 0.05, 2.22), (0.2, 0.05, 0.04))  # guarda
    m.limb(0, (-0.27, 0, 1.7), (-0.38, 0.2, 1.4), 0.065)  # brazo del escudo
    m.disc(0, (-0.42, 0.3, 1.4), 0.28, 0.05, rot=(90, 0, 0))  # escudo
    m.save()


for build in (superman, flash, batman, aquaman, ballena, linterna_verde, mujer_maravilla):
    build()
log("LISTO")
