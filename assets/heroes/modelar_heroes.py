# MODELOS LOW-POLY DE LA LIGA para el Proyecto 3 (Diego).
#
# Se ejecuta con Blender (no necesita abrir la ventana):
#     blender --background --python assets/heroes/modelar_heroes.py
#
# Cada personaje se arma con primitivas de Blender:
#   - el cuerpo con elipsoides (pecho, abdomen, cadera) y extremidades CÓNICAS unidas por
#     esferas en hombros, codos y rodillas (así no quedan huecos al doblarlas);
#   - las capas son una malla de "tela" que cae desde los hombros, se abre, rodea la espalda,
#     tiene pliegues y grosor (modificador Solidify);
#   - la pose se da con la posición de cada articulación (ver POSE_DE_PIE).
# Se une en un solo objeto con sombreado plano y se exporta a assets/heroes/<nombre>.obj
# (triangulado y con normales), con su .blend y dos LODs hechos con Decimate:
# <nombre>_lod1.obj (50 %) y <nombre>_lod2.obj (25 %).
#
# Ejes: en Blender el personaje mira hacia +Y y "arriba" es +Z. El exportador OBJ convierte
# (x, y, z) de Blender en (x, z, -y), así que en el archivo mira hacia -Z: lo que espera el juego.
#
# COLORES con una "textura de paleta": cada parte lleva una coordenada u distinta
# (u = (banda + 0.5) / bandas) y el HeroShader del juego elige el color de la paleta según u.
# Las paletas de aquí deben estar en el MISMO orden que en src/creatures.rs.
#
# Variables de entorno opcionales: HEROES_LOG (archivo de bitácora) y HEROES_PREVIEW
# (carpeta donde se guardan vistas previas renderizadas con Workbench).

import math
import os

import bpy
from mathutils import Vector

OUT = os.path.dirname(os.path.abspath(__file__))
LOG = os.environ.get("HEROES_LOG")
PREVIEW = os.environ.get("HEROES_PREVIEW")

SKIN = (0.93, 0.72, 0.58)
YELLOW = (1.0, 0.82, 0.15)
BLACK = (0.05, 0.05, 0.06)


def log(text):
    print(text)
    if LOG:
        with open(LOG, "a", encoding="utf-8") as f:
            f.write(text + "\n")


def rad(angles):
    return [math.radians(a) for a in angles]


def V(*xyz):
    return Vector(xyz)


class Model:
    """Un personaje en construcción: se le agregan partes y al final se exporta."""

    def __init__(self, name, colors):
        bpy.ops.wm.read_factory_settings(use_empty=True)
        self.name = name
        self.bands = len(colors)
        self.parts = []
        # Un material por color: no lo usa el juego, pero así el .blend se ve con colores.
        self.materials = []
        for i, c in enumerate(colors):
            mat = bpy.data.materials.new(f"{name}_{i}")
            mat.diffuse_color = (*c, 1.0)
            if mat.node_tree and mat.node_tree.nodes.get("Principled BSDF"):
                mat.node_tree.nodes["Principled BSDF"].inputs["Base Color"].default_value = (*c, 1.0)
            self.materials.append(mat)

    def _part(self, band):
        # Pinta TODA la parte con la misma u (para el juego) y su material (para Blender).
        obj = bpy.context.active_object
        mesh = obj.data
        if not mesh.uv_layers:
            mesh.uv_layers.new(name="UVMap")
        u = (band + 0.5) / self.bands
        for loop_uv in mesh.uv_layers.active.data:
            loop_uv.uv = (u, 0.5)
        mesh.materials.append(self.materials[band])
        self.parts.append(obj)
        return obj

    def _apply(self, obj, kind, **settings):
        """Agrega un modificador y lo aplica de una vez."""
        mod = obj.modifiers.new(kind, kind)
        for k, v in settings.items():
            setattr(mod, k, v)
        bpy.ops.object.modifier_apply(modifier=mod.name)

    def box(self, band, loc, size, rot=(0, 0, 0), bevel=0.0):
        bpy.ops.mesh.primitive_cube_add(size=1, location=loc, rotation=rad(rot))
        obj = bpy.context.active_object
        obj.scale = size
        if bevel > 0:
            bpy.ops.object.transform_apply(location=False, rotation=False, scale=True)
            self._apply(obj, "BEVEL", width=bevel, segments=1)
        return self._part(band)

    def ball(self, band, loc, radius, scale=(1, 1, 1), segments=8, rings=6):
        bpy.ops.mesh.primitive_uv_sphere_add(segments=segments, ring_count=rings, radius=radius, location=loc)
        bpy.context.active_object.scale = scale
        return self._part(band)

    def disc(self, band, loc, radius, depth, rot=(0, 0, 0), scale=(1, 1, 1), vertices=12):
        bpy.ops.mesh.primitive_cylinder_add(vertices=vertices, radius=radius, depth=depth, location=loc, rotation=rad(rot))
        bpy.context.active_object.scale = scale
        return self._part(band)

    def cone(self, band, loc, r1, r2, depth, rot=(0, 0, 0), vertices=8):
        bpy.ops.mesh.primitive_cone_add(vertices=vertices, radius1=r1, radius2=r2, depth=depth, location=loc, rotation=rad(rot))
        return self._part(band)

    def taper(self, band, a, b, r1, r2, vertices=8):
        """Cono truncado del punto a (radio r1) al punto b (radio r2): brazos, piernas, mangos."""
        a, b = Vector(a), Vector(b)
        d = b - a
        rot = d.to_track_quat("Z", "Y").to_euler()
        bpy.ops.mesh.primitive_cone_add(
            vertices=vertices, radius1=r1, radius2=r2, depth=d.length, location=(a + b) / 2, rotation=rot
        )
        return self._part(band)

    def cloth(self, band, top_z, length, back_y, w_top, w_bot, flare, bulge, folds, cols=5, rows=6, thickness=0.025):
        """Capa: una malla que cae desde top_z, se abre de w_top a w_bot, se aleja de la espalda
        (flare), rodea el cuerpo (bulge) y ondula (folds). Solidify le da grosor."""
        bpy.ops.mesh.primitive_grid_add(x_subdivisions=cols, y_subdivisions=rows, size=1.0)
        obj = bpy.context.active_object
        for v in obj.data.vertices:
            u = v.co.x + 0.5  # 0 a 1 de un lado al otro
            s = 0.5 - v.co.y  # 0 arriba, 1 abajo
            width = w_top + (w_bot - w_top) * s
            x = (u - 0.5) * width
            y = back_y - flare * s * s - bulge * (1 - (2 * u - 1) ** 2) - folds * math.sin(u * math.pi * 4) * s
            v.co = (x, y, top_z - s * length)
        self._apply(obj, "SOLIDIFY", thickness=thickness)
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

    def save(self, rotate_x=0.0):
        # Unir todas las partes en un solo objeto y dejar las transformaciones aplicadas.
        bpy.ops.object.select_all(action="DESELECT")
        for p in self.parts:
            p.select_set(True)
        bpy.context.view_layer.objects.active = self.parts[0]
        bpy.ops.object.join()
        obj = bpy.context.active_object
        obj.name = self.name
        obj.rotation_euler = (math.radians(rotate_x), 0, 0)  # p. ej. Superman: de pie → volando
        bpy.ops.object.transform_apply(location=True, rotation=True, scale=True)
        bpy.ops.object.shade_flat()

        report_shape(obj)
        tris = sum(len(p.vertices) - 2 for p in obj.data.polygons)
        bpy.ops.wm.save_as_mainfile(filepath=os.path.join(OUT, self.name + ".blend"))
        self._export(self.name)
        render_preview(obj, self.name)
        counts = [tris]
        # LODs: Decimate reduce los triángulos a la mitad cada vez.
        for k in (1, 2):
            self._apply(obj, "DECIMATE", ratio=0.5)
            counts.append(sum(len(p.vertices) - 2 for p in obj.data.polygons))
            self._export(f"{self.name}_lod{k}")
        log(f"{self.name}: triangulos LOD0/1/2 = {counts}")


def report_shape(obj):
    """Medidas como las ve el juego: centro = promedio de vértices y radio = el más lejano.
    Sirven para las constantes FEET y WHALE_BACK de creatures.rs."""
    pts = [v.co for v in obj.data.vertices]
    c = sum(pts, Vector()) / len(pts)
    r = max((p - c).length for p in pts)
    feet = (c.z - min(p.z for p in pts)) / r
    back = [p.z for p in pts if abs(p.x) < 0.12 and abs(p.y - c.y) < 0.3]
    top = (max(back) - c.z) / r if back else 0.0
    log(f"{obj.name}: pies a {feet:.2f} del radio bajo el centro; lomo/cabeza a {top:.2f}")


def render_preview(obj, name):
    """Vista previa con Workbench desde adelante-izquierda y atrás-derecha (si HEROES_PREVIEW)."""
    if not PREVIEW:
        return
    scene = bpy.context.scene
    scene.render.engine = "BLENDER_WORKBENCH"
    scene.display.shading.light = "STUDIO"
    scene.display.shading.color_type = "MATERIAL"
    scene.render.resolution_x, scene.render.resolution_y = 300, 360
    if scene.world is None:
        scene.world = bpy.data.worlds.new("Mundo")
    scene.world.color = (0.12, 0.12, 0.15)
    box = [obj.matrix_world @ Vector(corner) for corner in obj.bound_box]
    center = sum(box, Vector()) / 8
    size = max((b - center).length for b in box)
    cam = bpy.data.objects.new("camara", bpy.data.cameras.new("camara"))
    scene.collection.objects.link(cam)
    scene.camera = cam
    for k, (azimuth, elevation) in enumerate([(35, 12), (215, 15)]):
        a, e = math.radians(azimuth), math.radians(elevation)
        cam.location = center + Vector((math.sin(a) * math.cos(e), math.cos(a) * math.cos(e), math.sin(e))) * size * 3.4
        cam.rotation_euler = (center - cam.location).to_track_quat("-Z", "Y").to_euler()
        scene.render.filepath = os.path.join(PREVIEW, f"{name}_{k}.png")
        bpy.ops.render.render(write_still=True)
    bpy.data.objects.remove(cam)


# Pose de pie (frente +Y, arriba +Z, ~1.9 de alto). "_r" = derecha del personaje (+X).
POSE_DE_PIE = {
    "pelvis": (0, 0, 0.97), "abdomen": (0, 0, 1.12), "chest": (0, 0, 1.33), "head": (0, 0.01, 1.71),
    "sh_r": (0.25, 0, 1.46), "el_r": (0.31, -0.02, 1.18), "wr_r": (0.33, 0.02, 0.93),
    "sh_l": (-0.25, 0, 1.46), "el_l": (-0.31, -0.02, 1.18), "wr_l": (-0.33, 0.02, 0.93),
    "hip_r": (0.11, 0, 0.94), "kn_r": (0.12, 0.02, 0.52), "an_r": (0.12, 0, 0.1),
    "hip_l": (-0.11, 0, 0.94), "kn_l": (-0.12, 0.02, 0.52), "an_l": (-0.12, 0, 0.1),
}
UPPER_BODY = ("abdomen", "chest", "head", "sh_r", "el_r", "wr_r", "sh_l", "el_l", "wr_l")


def humanoid(m, col, changes=None, lean=0.0, lift=0.0, chest=(0.25, 0.155, 0.21), hips=0.19, toes=(0, 0)):
    """Cuerpo humano a partir de la pose de pie con `changes` (articulaciones movidas).
    col: banda de cada parte. lean: inclinación del torso hacia adelante. lift: subir todo.
    toes: ángulo de cada pie en grados (0 = plano, -90 = de puntas hacia abajo)."""
    pose = dict(POSE_DE_PIE)
    pose.update(changes or {})
    P = {}
    for k, p in pose.items():
        p = Vector(p) + V(0, 0, lift)
        if k in UPPER_BODY:  # el torso se inclina: más arriba = más adelante
            p += V(0, (p.z - 0.95 - lift) * lean, 0)
        P[k] = p
    get = lambda k, default: col.get(k, col[default])

    m.ball(get("pelvis", "body"), P["pelvis"], 1.0, scale=(hips, 0.13, 0.12), segments=8, rings=6)
    m.ball(get("abdomen", "body"), P["abdomen"], 1.0, scale=(0.185, 0.125, 0.17), segments=8, rings=6)
    m.ball(get("chest", "body"), P["chest"], 1.0, scale=chest, segments=10, rings=7)
    m.taper(get("neck", "skin"), P["chest"] + V(0, 0, 0.16), P["head"] - V(0, 0, 0.06), 0.06, 0.052)
    m.ball(get("head", "skin"), P["head"], 0.125, scale=(0.92, 1.0, 1.12), segments=10, rings=7)
    for s, toe in zip("rl", toes):
        sh, el, wr = P["sh_" + s], P["el_" + s], P["wr_" + s]
        m.ball(get("shoulder", "body"), sh, 0.085, segments=6, rings=4)
        m.taper(get("upper_arm", "body"), sh, el, 0.075, 0.058)
        m.ball(get("elbow", "body"), el, 0.058, segments=6, rings=4)
        m.taper(get("forearm", "body"), el, wr, 0.058, 0.044)
        m.ball(get("hand", "skin"), wr + (wr - el).normalized() * 0.055, 0.055, scale=(0.85, 0.9, 1.15), segments=6, rings=4)
        hip, kn, an = P["hip_" + s], P["kn_" + s], P["an_" + s]
        m.taper(get("thigh", "body"), hip, kn, 0.1, 0.072)
        m.ball(get("knee", "body"), kn, 0.07, segments=6, rings=4)
        m.taper(get("shin", "body"), kn, an, 0.07, 0.048)
        t = math.radians(toe)
        foot = an + V(0, math.cos(t), math.sin(t)) * 0.08 + V(0, 0, -0.03 * math.cos(t))
        m.box(get("foot", "body"), foot, (0.095, 0.23, 0.075), rot=(toe, 0, 0), bevel=0.02)
    return P


def boots(m, band, P, height=0.22, r=0.06):
    """Botas: un cono un poco más ancho que la espinilla, del tobillo hacia arriba."""
    for s in "rl":
        kn, an = P["kn_" + s], P["an_" + s]
        top = an + (kn - an).normalized() * height
        m.taper(band, an - (kn - an).normalized() * 0.02, top, r, r * 1.08)


def gloves(m, band, P, length=0.14, r=0.052):
    for s in "rl":
        el, wr = P["el_" + s], P["wr_" + s]
        m.taper(band, wr + (wr - el).normalized() * 0.01, wr - (wr - el).normalized() * length, r * 0.9, r * 1.15)


def belt(m, band, P, width=0.2):
    m.disc(band, P["pelvis"] + V(0, 0, 0.07), width, 0.05, scale=(1, 0.68, 1), vertices=14)


def chest_front(P, depth=0.158):
    """Punto al frente del pecho, donde va el emblema."""
    return P["chest"] + V(0, depth, 0.02)


def superman():
    m = Model("superman", [(0.10, 0.25, 0.80), (0.85, 0.08, 0.06), SKIN, YELLOW, BLACK])
    blue, red, skin, yellow, black = range(5)
    # Se modela DE PIE con el brazo derecho arriba y al final se gira -90° en X: queda volando
    # con la cabeza y el puño hacia adelante (+Y), el pecho hacia abajo y la capa arriba.
    P = humanoid(
        m,
        {"body": blue, "skin": skin, "pelvis": red, "foot": red},
        {
            "el_r": (0.27, 0.03, 1.74), "wr_r": (0.27, 0.05, 1.99),
            "el_l": (-0.3, -0.05, 1.2), "wr_l": (-0.31, -0.06, 0.95),
            "kn_l": (-0.1, -0.07, 0.53), "an_l": (-0.1, -0.12, 0.13),
            "kn_r": (0.1, 0.0, 0.52), "an_r": (0.1, -0.02, 0.1),
        },
        toes=(-100, -100),
    )
    boots(m, red, P)
    belt(m, yellow, P)
    front = chest_front(P)
    m.box(yellow, front, (0.15, 0.02, 0.15), rot=(0, 45, 0))  # escudo
    m.box(red, front + V(0, 0.008, 0), (0.1, 0.02, 0.1), rot=(0, 45, 0))
    m.box(yellow, front + V(0, 0.016, 0), (0.06, 0.02, 0.012), rot=(0, -30, 0))  # trazo de la "S"
    m.ball(black, P["head"] + V(0, -0.015, 0.05), 0.125, scale=(0.96, 1.0, 0.78), segments=10, rings=6)  # pelo
    m.cone(black, P["head"] + V(0, 0.1, 0.11), 0.02, 0.0, 0.06, rot=(60, 0, 0), vertices=5)  # rizo
    m.cloth(red, top_z=1.5, length=1.2, back_y=-0.17, w_top=0.5, w_bot=0.8, flare=0.3, bulge=0.06, folds=0.02)
    m.save(rotate_x=-90)


def flash():
    m = Model("flash", [(0.80, 0.06, 0.04), YELLOW, SKIN])
    red, yellow, skin = range(3)
    P = humanoid(
        m,
        {"body": red, "skin": red, "hand": red},
        {
            "kn_r": (0.12, 0.33, 0.62), "an_r": (0.12, 0.27, 0.18),
            "kn_l": (-0.11, -0.12, 0.55), "an_l": (-0.11, -0.48, 0.42),
            "el_r": (0.3, -0.24, 1.25), "wr_r": (0.32, -0.1, 1.05),
            "el_l": (-0.3, 0.2, 1.25), "wr_l": (-0.3, 0.42, 1.37),
        },
        lean=0.28,
        toes=(0, -45),
    )
    boots(m, yellow, P, height=0.2)
    for s in "rl":
        m.box(yellow, P["an_" + s] + V(0, 0.0, -0.02), (0.1, 0.12, 0.05), bevel=0.01)
    belt(m, yellow, P)
    front = chest_front(P)
    m.disc(yellow, front, 0.07, 0.02, rot=(90, 0, 0), vertices=12)  # emblema
    m.box(red, front + V(0, 0.012, 0), (0.025, 0.01, 0.1), rot=(0, 25, 0))  # rayo
    # Cara descubierta (la capucha es la cabeza roja) y alitas en las orejas.
    m.ball(skin, P["head"] + V(0, 0.055, -0.04), 0.085, scale=(0.95, 0.75, 0.7), segments=8, rings=5)
    for x in (0.115, -0.115):
        m.box(yellow, P["head"] + V(x, -0.02, 0.04), (0.02, 0.1, 0.035), rot=(-25, 0, 0), bevel=0.005)
    m.save()


def batman():
    m = Model("batman", [(0.30, 0.30, 0.34), BLACK, YELLOW, SKIN])
    gray, black, yellow, skin = range(4)
    P = humanoid(
        m,
        {"body": gray, "skin": skin, "head": black, "hand": black, "foot": black},
        {
            "kn_r": (0.15, 0.02, 0.52), "an_r": (0.18, 0, 0.1),
            "kn_l": (-0.15, 0.02, 0.52), "an_l": (-0.18, 0, 0.1),
            "el_r": (0.33, -0.03, 1.18), "wr_r": (0.35, 0.03, 0.94),
            "el_l": (-0.33, -0.03, 1.18), "wr_l": (-0.35, 0.03, 0.94),
        },
        chest=(0.27, 0.16, 0.22),
    )
    boots(m, black, P, height=0.26)
    gloves(m, black, P)
    for s, x in (("r", 1), ("l", -1)):  # aletas de los guantes
        m.cone(black, P["wr_" + s] - (P["wr_" + s] - P["el_" + s]).normalized() * 0.08 + V(0.05 * x, 0, 0), 0.02, 0, 0.07, rot=(0, 90 * x, 0), vertices=4)
    belt(m, yellow, P, width=0.205)
    front = chest_front(P, 0.165)
    m.disc(yellow, front, 0.1, 0.02, rot=(90, 0, 0), scale=(1, 1, 0.6), vertices=14)  # óvalo
    m.box(black, front + V(0, 0.012, 0), (0.15, 0.01, 0.035), bevel=0.005)  # alas del murciélago
    m.box(black, front + V(0, 0.012, -0.015), (0.05, 0.01, 0.04))
    # Quijada descubierta y orejas de la máscara.
    m.ball(skin, P["head"] + V(0, 0.05, -0.055), 0.08, scale=(0.9, 0.8, 0.65), segments=8, rings=5)
    for x in (0.055, -0.055):
        m.cone(black, P["head"] + V(x, 0, 0.17), 0.03, 0.0, 0.13, vertices=5)
    # Capa larga: hombros y tela que cae casi hasta los tobillos.
    m.box(black, P["chest"] + V(0, -0.04, 0.17), (0.6, 0.28, 0.07), bevel=0.03)
    m.cloth(black, top_z=1.52, length=1.38, back_y=-0.16, w_top=0.56, w_bot=1.05, flare=0.12, bulge=0.07, folds=0.025, rows=7)
    m.save()


def aquaman():
    m = Model("aquaman", [(0.95, 0.55, 0.10), (0.10, 0.55, 0.25), SKIN, (0.95, 0.80, 0.35), (1.0, 0.75, 0.2)])
    orange, green, skin, blond, gold = range(5)
    P = humanoid(
        m,
        {"body": green, "skin": skin, "chest": orange, "abdomen": orange, "shoulder": orange, "upper_arm": orange, "hand": green},
        {
            "el_r": (0.31, 0.1, 1.2), "wr_r": (0.33, 0.2, 1.14),
            "el_l": (-0.4, -0.02, 1.2), "wr_l": (-0.23, 0.03, 1.02),
            "an_r": (0.15, 0, 0.1), "an_l": (-0.15, 0, 0.1),
        },
        chest=(0.26, 0.16, 0.22),
    )
    belt(m, gold, P)
    # Cabello largo y barba rubios.
    m.ball(blond, P["head"] + V(0, -0.025, 0.035), 0.13, scale=(1.0, 1.0, 1.0), segments=10, rings=6)
    m.taper(blond, P["head"] + V(0, -0.06, 0.0), P["head"] + V(0, -0.09, -0.22), 0.1, 0.07)
    m.ball(blond, P["head"] + V(0, 0.055, -0.075), 0.065, scale=(1.1, 0.8, 0.9), segments=8, rings=5)
    # Tridente dorado en la mano derecha.
    hand = P["wr_r"] + V(0, 0.05, 0)
    m.taper(gold, (hand.x, hand.y, 0.2), (hand.x, hand.y, 2.1), 0.022, 0.022, vertices=6)
    m.box(gold, (hand.x, hand.y, 2.1), (0.28, 0.035, 0.035), bevel=0.008)
    for dx in (-0.12, 0.0, 0.12):
        m.cone(gold, (hand.x + dx, hand.y, 2.25), 0.03, 0.0, 0.26 if dx else 0.32, vertices=6)
    m.save()


def ballena():
    m = Model("ballena", [(0.20, 0.28, 0.45), (0.70, 0.76, 0.82), BLACK, (0.3, 0.95, 1.0)])
    dark, belly, black, cyan = range(4)
    m.ball(dark, (0, 0, 0), 1.0, scale=(0.55, 1.45, 0.5), segments=16, rings=10)  # cuerpo
    m.ball(belly, (0, 0.15, -0.13), 1.0, scale=(0.5, 1.25, 0.38), segments=14, rings=8)  # vientre
    m.taper(dark, (0, -1.15, 0.02), (0, -1.95, 0.09), 0.3, 0.09, vertices=10)  # base de la cola
    m.cone(dark, (0, -0.55, 0.5), 0.12, 0.0, 0.18, vertices=6)  # aleta dorsal
    for s in (1, -1):
        m.box(dark, (0.33 * s, -2.05, 0.09), (0.62, 0.28, 0.05), rot=(0, 0, 24 * s), bevel=0.02)  # cola
        m.box(dark, (0.6 * s, 0.45, -0.25), (0.55, 0.2, 0.04), rot=(0, 25 * s, -30 * s), bevel=0.02)  # aletas
        m.ball(black, (0.43 * s, 0.95, -0.02), 0.055, segments=6, rings=4)  # ojos
        m.box(black, (0.39 * s, 1.0, -0.13), (0.02, 0.45, 0.015), rot=(0, 0, 6 * s))  # boca
    # Manchas que brillan (es una ballena espacial) en el lomo y los costados.
    for y in (-0.9, -0.45, 0.0, 0.45, 0.9):
        h = math.sqrt(max(0.0, 1 - (y / 1.45) ** 2))
        m.ball(cyan, (0, y, 0.5 * h), 0.05, segments=6, rings=4)
        for s in (1, -1):
            m.ball(cyan, (0.5 * h * 0.92 * s, y + 0.2, 0.12), 0.04, segments=6, rings=4)
    m.save()


def linterna_verde():
    m = Model("linterna_verde", [(0.05, 0.45, 0.15), BLACK, SKIN, (0.95, 0.95, 0.95)])
    green, black, skin, white = range(4)
    P = humanoid(
        m,
        {"body": green, "skin": skin, "shoulder": black, "upper_arm": black, "pelvis": black, "hand": white},
        {
            "el_r": (0.28, 0.3, 1.45), "wr_r": (0.29, 0.56, 1.45),
            "el_l": (-0.33, -0.02, 1.2), "wr_l": (-0.27, 0.14, 1.08),
            "kn_r": (0.11, 0.06, 0.53), "an_r": (0.11, -0.03, 0.13),
            "kn_l": (-0.11, 0.13, 0.6), "an_l": (-0.11, 0.02, 0.22),
        },
        toes=(-60, -60),
    )
    m.box(black, P["chest"] + V(0, 0, 0.12), (0.5, 0.3, 0.09), bevel=0.03)  # franja negra de los hombros
    front = chest_front(P)
    m.disc(white, front, 0.085, 0.02, rot=(90, 0, 0), vertices=14)  # emblema
    m.disc(green, front + V(0, 0.012, 0), 0.055, 0.02, rot=(90, 0, 0), vertices=12)
    m.box(green, front + V(0, 0.02, 0), (0.13, 0.01, 0.02))  # barras del emblema
    m.box(green, front + V(0, 0.02, 0), (0.02, 0.01, 0.13))
    m.ball(black, P["head"] + V(0, -0.02, 0.05), 0.125, scale=(0.96, 1.0, 0.75), segments=10, rings=6)  # pelo
    m.box(green, P["head"] + V(0, 0.1, 0.01), (0.2, 0.04, 0.055), bevel=0.01)  # antifaz
    for x in (0.045, -0.045):
        m.box(white, P["head"] + V(x, 0.122, 0.012), (0.04, 0.01, 0.022))  # ojos
    fist = P["wr_r"] + (P["wr_r"] - P["el_r"]).normalized() * 0.07
    m.disc(green, fist + V(0, 0.0, 0.05), 0.035, 0.03, rot=(0, 90, 0), vertices=10)  # anillo
    m.save()


def mujer_maravilla():
    m = Model("mujer_maravilla", [(0.90, 0.88, 0.82)])
    stone = 0
    base = 0.36  # la estatua está sobre un pedestal
    m.box(stone, (0, 0, 0.12), (1.0, 1.0, 0.24), bevel=0.03)
    m.box(stone, (0, 0, 0.3), (0.78, 0.78, 0.12), bevel=0.02)
    P = humanoid(
        m,
        {"body": stone, "skin": stone},
        {
            "el_r": (0.32, 0.03, 1.72), "wr_r": (0.36, 0.05, 1.98),
            "el_l": (-0.3, 0.13, 1.25), "wr_l": (-0.26, 0.33, 1.28),
            "kn_r": (0.15, 0.06, 0.52), "an_r": (0.2, 0.05, 0.1),
            "kn_l": (-0.12, -0.04, 0.52), "an_l": (-0.15, -0.08, 0.1),
        },
        lift=base,
        chest=(0.22, 0.14, 0.2),
        hips=0.2,
    )
    m.cone(stone, P["pelvis"] + V(0, 0, -0.07), 0.27, 0.2, 0.22, vertices=12)  # falda
    boots(m, stone, P, height=0.3)
    # Cabello largo, tiara con estrella.
    m.ball(stone, P["head"] + V(0, -0.02, 0.03), 0.13, segments=10, rings=6)
    m.taper(stone, P["head"] + V(0, -0.05, 0.0), P["head"] + V(0, -0.1, -0.35), 0.11, 0.08)
    m.box(stone, P["head"] + V(0, 0.09, 0.08), (0.17, 0.03, 0.035))
    m.cone(stone, P["head"] + V(0, 0.115, 0.11), 0.025, 0.0, 0.07, vertices=5)
    # Espada en alto y escudo.
    hand = P["wr_r"] + (P["wr_r"] - P["el_r"]).normalized() * 0.05
    m.box(stone, hand + V(0, 0, 0.08), (0.2, 0.05, 0.035), bevel=0.008)  # guarda
    m.box(stone, hand + V(0, 0, 0.5), (0.05, 0.015, 0.75), bevel=0.005)  # hoja
    m.cone(stone, hand + V(0, 0, 0.95), 0.035, 0.0, 0.12, vertices=4)  # punta
    m.disc(stone, P["wr_l"] + V(0, 0.07, 0), 0.22, 0.04, rot=(90, 0, 0), vertices=14)  # escudo
    m.ball(stone, P["wr_l"] + V(0, 0.1, 0), 0.05, scale=(1, 0.5, 1), segments=6, rings=4)
    m.save()


for build in (superman, flash, batman, aquaman, ballena, linterna_verde, mujer_maravilla):
    build()
log("LISTO")
