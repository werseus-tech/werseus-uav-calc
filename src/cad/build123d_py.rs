// SPDX-License-Identifier: AGPL-3.0-or-later
//! Static Python part of the fast CAD backend (build123d on OpenCascade). The generated
//! script is `header + JOB = json.loads(<JSON string literal>) + PY_LIB + main(JOB)`: every
//! value (names, paths, numbers) reaches Python only through that single JSON literal, so no
//! user text is ever interpolated into code.

/// Python library: part builders, DXF sections, assembly checks and the job loop.
pub(crate) const PY_LIB: &str = r#"
import json, math, os, sys, time, traceback

STAGE = "__WERSEUS_STAGE__:"
PART_TAG = "__WERSEUS_FAST_PART__:"
RESULT_START = "__WERSEUS_CAD_JSON_START__"
RESULT_END = "__WERSEUS_CAD_JSON_END__"


def stage(sid, i=None, n=None):
    # Fixed-id progress line for the UI (never carries user data).
    line = STAGE + sid
    if i is not None and n is not None:
        line += ":" + str(i) + "/" + str(n)
    print(line, flush=True)


stage("import")
from build123d import (Axis, Compound, Edge, ExportDXF, Face, Location, Plane, Solid, Vector,
                       Wire, export_brep, export_step, export_stl, import_brep)
from OCP.BRepAlgoAPI import BRepAlgoAPI_Common
from OCP.BRepBuilderAPI import BRepBuilderAPI_MakeEdge
from OCP.BRepExtrema import BRepExtrema_DistShapeShape
from OCP.BRepGProp import BRepGProp
from OCP.GProp import GProp_GProps
from OCP.Geom import Geom_BSplineCurve
from OCP.TopLoc import TopLoc_Location
try:  # OCP >= 7.9 / 8: NCollection templates live in OCP.collections
    from OCP.collections import Array1_double as _ArrReal, Array1_gp_Pnt as _ArrPnt
    from OCP.collections import Array1_int as _ArrInt, List_TopoDS_Shape as _ShapeList
except ImportError:  # older OCP
    from OCP.TColgp import TColgp_Array1OfPnt as _ArrPnt
    from OCP.TColStd import TColStd_Array1OfInteger as _ArrInt, TColStd_Array1OfReal as _ArrReal
    from OCP.TopTools import TopTools_ListOfShape as _ShapeList
from OCP.gp import gp_Pnt, gp_Trsf

MIRROR_PLANES = {0: Plane.YZ, 1: Plane.XZ, 2: Plane.XY}


def vec(p):
    return Vector(float(p[0]), float(p[1]), float(p[2]))


def gprops(shape):
    """(volume mm3, [cx, cy, cz]) with the adaptive integrator: the default OCC volume
    integral is several percent off on ruled B-spline lofts."""
    w = shape.wrapped if hasattr(shape, "wrapped") else shape
    pr = GProp_GProps()
    try:
        BRepGProp.VolumeProperties_s(w, pr, 1e-6)
    except Exception:
        pr = GProp_GProps()
        BRepGProp.VolumeProperties_s(w, pr)
    c = pr.CentreOfMass()
    return abs(float(pr.Mass())), [float(c.X()), float(c.Y()), float(c.Z())]


def area_of(w):
    pr = GProp_GProps()
    BRepGProp.SurfaceProperties_s(w, pr)
    return abs(float(pr.Mass()))


def mat_apply(m, p):
    x, y, z = p
    return (m[0] * x + m[1] * y + m[2] * z + m[3],
            m[4] * x + m[5] * y + m[6] * z + m[7],
            m[8] * x + m[9] * y + m[10] * z + m[11])


def location(m):
    t = gp_Trsf()
    t.SetValues(*[float(v) for v in m[:12]])
    return Location(TopLoc_Location(t))


def interp_wire(vs):
    """Section through the points (one interpolating B-spline, like the FreeCAD path); an
    open trailing edge is closed with a straight line."""
    e = Edge.make_spline([vec(v) for v in vs])
    edges = [e]
    if (vec(vs[0]) - vec(vs[-1])).length > 1e-9:
        edges.append(Edge.make_line(vec(vs[-1]), vec(vs[0])))
    return Wire(edges)


def bspline_wire(poles, knots, mults):
    """Clamped cubic B-spline with the given control points (inner skin section)."""
    P = _ArrPnt(1, len(poles))
    for i, p in enumerate(poles):
        P.SetValue(i + 1, gp_Pnt(float(p[0]), float(p[1]), float(p[2])))
    K = _ArrReal(1, len(knots))
    for i, k in enumerate(knots):
        K.SetValue(i + 1, float(k))
    M = _ArrInt(1, len(mults))
    for i, k in enumerate(mults):
        M.SetValue(i + 1, int(k))
    curve = Geom_BSplineCurve(P, K, M, 3, False)
    return Wire([Edge(BRepBuilderAPI_MakeEdge(curve).Edge())])


def placed(pl, pts2):
    """Section placement [bx, by, yaw_rad, z]: rotate (x, y) by yaw, then translate."""
    bx, by, yaw, z = pl
    c, s = math.cos(yaw), math.sin(yaw)
    return [(bx + x * c - y * s, by + x * s + y * c, z) for x, y in pts2]


def box_min(x0, y0, z0, lx, ly, lz):
    return Solid.make_box(lx, ly, lz, Plane(origin=(x0, y0, z0)))


def cyl(r, h, origin, direction=(0, 0, 1)):
    return Solid.make_cylinder(r, h, Plane(origin=vec(origin), z_dir=vec(direction)))


def cut_all(shape, tools):
    tools = [t for t in tools if t is not None]
    return shape.cut(*tools) if tools else shape


def surf_plug(sp):
    """The surface's exact slice [z0, z1] in the host part's frame (wing/tail slot, saddle)."""
    def sec(z):
        s = abs(z)
        c = sp["c_r"] + (sp["c_t"] - sp["c_r"]) * s / sp["span"]
        x0 = s * math.tan(math.radians(sp["sweep"]))
        return interp_wire([mat_apply(sp["m"], (x0 + q[0] * c, q[1] * c, z)) for q in sp["pts"]])
    return Solid.make_loft([sec(sp["z0"]), sec(sp["z1"])], ruled=True)


def rev_xz(loop):
    """Closed (x, r) polygon in the XZ plane revolved 360 deg about X."""
    w = Wire.make_polygon([Vector(float(x), 0.0, float(r)) for x, r in loop], close=True)
    return Solid.revolve(Face(w), 360, Axis.X)


# ---------------------------------------------------------------- part builders

def build_surface(sp):
    root = interp_wire(placed(sp["root"][1:], [(q[0] * sp["root"][0], q[1] * sp["root"][0]) for q in sp["pts"]]))
    tip = interp_wire(placed(sp["tip"][1:], [(q[0] * sp["tip"][0], q[1] * sp["tip"][0]) for q in sp["pts"]]))
    shape = Solid.make_loft([root, tip], ruled=True)
    tools = []
    for bay in sp["bays"]:
        wa = bspline_wire(placed(bay["a"]["pl"], bay["a"]["poles"]), sp["knots"], sp["mults"])
        wb = bspline_wire(placed(bay["b"]["pl"], bay["b"]["poles"]), sp["knots"], sp["mults"])
        tools.append(Solid.make_loft([wa, wb], ruled=True))
    s = sp["spar"]
    if s:
        tools.append(cyl(s["r"], s["len"], s["start"], s["dir"]))
    return cut_all(shape, tools)


def build_fuselage(sp):
    shape = rev_xz(sp["outer"])
    big = 4.0 * sp["r_max"]
    pad = sp["pad"]
    tools = []
    if pad:
        tools.append(box_min(pad["x0"], -big, -big, pad["x1"] - pad["x0"], 2 * big, big + pad["z"]))
    if sp["inner"]:
        void = rev_xz(sp["inner"])
        vt = [rev_xz(f) for f in sp["formers"]]
        if pad:
            w = sp["wall"]
            vt.append(box_min(pad["x0"] - w, -big, -big, pad["x1"] - pad["x0"] + 2 * w, 2 * big, big + pad["z"] + w))
        tools.append(cut_all(void, vt))
    tools += [surf_plug(p) for p in sp["plugs"]]
    return cut_all(shape, tools)


def build_clamp(sp):
    w, ro = sp["width"], sp["bore"] / 2 + sp["wall"]
    body = cyl(ro, w, (-w / 2, 0, 0), (1, 0, 0)).fuse(box_min(-w / 2, -ro, 0, w, 2 * ro, sp["pillar_top"]))
    tools = [cyl(sp["bore"] / 2, w + 2, (-w / 2 - 1, 0, 0), (1, 0, 0)),
             box_min(-w / 2 - 1, -sp["slit"] / 2, -(ro + 1), w + 2, sp["slit"], ro + 1)]
    if sp["saddle"]:
        tools.append(surf_plug(sp["saddle"]))
    return cut_all(body, tools)


def build_tube(sp):
    shape = cyl(sp["od"] / 2, sp["length"], (0, 0, 0))
    if 0 < sp["id"] < sp["od"]:
        shape = shape.cut(cyl(sp["id"] / 2, sp["length"] + 2, (0, 0, -1)))
    return shape


def build_motor_mount(sp):
    s, t = sp["size"], sp["t"]
    shape = box_min(-s / 2, -s / 2, 0, s, s, t)
    tools = [cyl(sp["shaft"] / 2, t + 2, (0, 0, -1))]
    for dx, dy in ((-1, 0), (1, 0), (0, -1), (0, 1)):
        tools.append(cyl(sp["bolt"] / 2, t + 4, (dx * sp["pattern"] / 2, dy * sp["pattern"] / 2, -2)))
    for dx, dy in ((-1, -1), (-1, 1), (1, -1), (1, 1)):
        tools.append(cyl(sp["fw_bolt"] / 2, t + 4, (dx * sp["fw_spacing"] / 2, dy * sp["fw_spacing"] / 2, -2)))
    return cut_all(shape, tools)


def build_arm(sp):
    L, w, t = sp["length"], sp["width"], sp["t"]
    xm = L - w / 2
    shape = box_min(0, -w / 2, 0, xm, w, t).fuse(cyl(w / 2, t, (xm, 0, 0)))
    tools = [cyl(sp["shaft"] / 2, t + 2, (xm, 0, -1))]
    for dx, dy in ((-1, -1), (-1, 1), (1, -1), (1, 1)):
        tools.append(cyl(sp["bolt"] / 2, t + 2, (xm + dx * sp["pattern"] / 2, dy * sp["pattern"] / 2, -1)))
    for i in range(2):
        tools.append(cyl(sp["ch_bolt"] / 2, t + 2, (sp["ch_x0"] + i * sp["ch_spacing"], 0, -1)))
    tools.append(box_min(40.0, -0.2 * w, -1, 0.45 * L, 0.4 * w, t + 2))
    return cut_all(shape, tools)


def build_plate(sp):
    t = sp["t"]
    shape = cyl(sp["r"], t, (0, 0, 0))
    tools = [cyl(sp["center"] / 2, t + 4, (0, 0, -2))]
    for dx, dy in ((-1, -1), (-1, 1), (1, -1), (1, 1)):
        tools.append(cyl(1.6, t + 4, (dx * sp["fc"] / 2, dy * sp["fc"] / 2, -2)))
    for a in sp["angles"]:
        ca, sa = math.cos(math.radians(a)), math.sin(math.radians(a))
        for r in sp["arm_r"]:
            tools.append(cyl(sp["ch_bolt"] / 2, t + 4, (r * ca, r * sa, -2)))
    return cut_all(shape, tools)


def build_leg(sp):
    tl, tw, tt = sp["tab_l"], sp["tab_w"], sp["tab_t"]
    s, c = math.sin(math.radians(sp["splay"])), math.cos(math.radians(sp["splay"]))
    end_y, end_z = sp["len"] * s, -tt - sp["len"] * c
    tab = box_min(-tl / 2, -tw / 2, -tt, tl, tw, tt)
    leg = cyl(sp["dia"] / 2, sp["len"], (0, 0, -tt), (0, s, -c))
    foot = box_min(-sp["foot_l"] / 2, end_y - sp["foot_w"] / 2, end_z - sp["foot_t"], sp["foot_l"], sp["foot_w"], sp["foot_t"])
    return tab.fuse(leg, foot)


BUILDERS = {
    "surface": build_surface, "fuselage": build_fuselage, "clamp": build_clamp, "tube": build_tube,
    "motor_mount": build_motor_mount, "arm": build_arm, "plate": build_plate, "leg": build_leg,
}


def write_dxf(shape, planes, path):
    """Planar sections (ribs, formers, plates) laid out side by side along X."""
    exp = ExportDXF()
    x_off = 0.0
    n = 0
    for pl in planes:
        plane = Plane(origin=vec(pl["o"]), x_dir=vec(pl["x"]), z_dir=vec(pl["z"]))
        try:
            sec = shape.intersect(Face.make_rect(1e5, 1e5, plane))
        except Exception:
            sec = None
        faces = list(sec.faces()) if sec is not None else []
        if not faces:
            continue
        local = [plane.to_local_coords(f) for f in faces]
        bb = Compound(local).bounding_box()
        dx = x_off - bb.min.X
        exp.add_shape([f.moved(Location((dx, 0, 0))) for f in local])
        x_off += bb.size.X + 10.0
        n += 1
    if n:
        exp.write(path)
    return n


def one_solid(shape):
    s = shape.solids()
    return s[0] if len(s) == 1 else shape


def pair_metrics(la, sa, lb, sb, tol):
    rec = {"a": la, "b": lb, "volume_mm3": -1.0, "area_mm2": 0.0, "dist_mm": -1.0, "error": None}
    try:
        d = BRepExtrema_DistShapeShape(sa.wrapped, sb.wrapped)
        if d.IsDone():
            rec["dist_mm"] = float(d.Value())
        else:
            rec["error"] = "dist: not done"
    except Exception as e:
        rec["error"] = "dist: " + str(e)
    if rec["dist_mm"] < 0.0 or rec["dist_mm"] <= tol:
        # Fuzzy common volume: faces closer than tol count as coincident, so a part seated in
        # its slot or a tube in its hole does not produce a spurious thin volume.
        try:
            op = BRepAlgoAPI_Common()
            l1 = _ShapeList(); l1.Append(sa.wrapped)
            l2 = _ShapeList(); l2.Append(sb.wrapped)
            op.SetArguments(l1); op.SetTools(l2); op.SetFuzzyValue(tol)
            op.Build()
            if not op.IsDone():
                raise RuntimeError("common failed")
            res = op.Shape()
            v, _c = gprops(res)
            rec["volume_mm3"] = v if v > 1e-9 else 0.0
            rec["area_mm2"] = area_of(res) if v > 1e-9 else 0.0
        except Exception as e:
            rec["error"] = "common: " + str(e)
    else:
        rec["volume_mm3"] = 0.0
    return rec


def run_assembly(asm, built, scratch, out_dir):
    tol = float(asm["tol"])
    items = asm["items"]
    placed_parts = []
    for k, it in enumerate(items):
        stage("load", k + 1, len(items))
        sh = built.get(it["stem"])
        if sh is None:
            p = os.path.join(scratch, it["stem"] + ".brep")
            if not os.path.exists(p):
                continue
            sh = one_solid(import_brep(p))
        if it["mirror_axis"] is not None:
            sh = sh.mirror(MIRROR_PLANES[int(it["mirror_axis"])])
        sh = sh.moved(location(it["m"]))
        sh.label = it["label"]
        placed_parts.append((it["label"], sh, float(it["density"])))
    n = len(placed_parts)
    boxes = [sh.bounding_box() for _l, sh, _r in placed_parts]
    pairs = []
    g = 2.0 * tol + 0.01
    for a in range(n):
        stage("pairs", a + 1, n)
        for b in range(a + 1, n):
            A, B = boxes[a], boxes[b]
            if (A.max.X + g < B.min.X or B.max.X + g < A.min.X or A.max.Y + g < B.min.Y
                    or B.max.Y + g < A.min.Y or A.max.Z + g < B.min.Z or B.max.Z + g < A.min.Z):
                continue
            pairs.append(pair_metrics(placed_parts[a][0], placed_parts[a][1], placed_parts[b][0], placed_parts[b][1], tol))
    stage("mass")
    mass = 0.0
    mx = my = mz = 0.0
    total_vol = 0.0
    for _l, sh, rho in placed_parts:
        v, c = gprops(sh)
        m = v * 1e-9 * rho
        mass += m
        mx += m * c[0]; my += m * c[1]; mz += m * c[2]
        total_vol += v
    cg = [mx / mass, my / mass, mz / mass] if mass > 0 else [0.0, 0.0, 0.0]
    stage("export")
    step_files = []
    if asm["step"] and placed_parts:
        p = os.path.join(out_dir, asm["name"] + ".step")
        export_step(Compound(children=[sh for _l, sh, _r in placed_parts]), p)
        step_files.append(p)
    return {
        "ok": True, "status": "ok", "part": asm["name"], "valid": True,
        "volume_mm3": total_vol, "param_update_ok": False, "num_segments": n,
        "fcstd_path": "", "step_files": step_files, "stl_files": [],
        "error": None, "traceback": None, "mass_kg": mass, "cg_mm": cg, "pairs": pairs,
    }


def main(job):
    out_dir = job["out_dir"]
    scratch = job["scratch_dir"]
    os.makedirs(out_dir, exist_ok=True)
    os.makedirs(scratch, exist_ok=True)
    fm = job["formats"]
    built = {}
    parts = job["parts"]
    for i, part in enumerate(parts):
        stage("part", i + 1, len(parts))
        stem = part["stem"]
        spec = part["spec"]
        rec = {"key": part["key"], "stem": stem, "ok": False, "valid": False, "solids": 0,
               "volume_mm3": 0.0, "files": [], "error": None, "traceback": None, "secs": 0.0}
        t0 = time.time()
        try:
            shape = one_solid(BUILDERS[spec["kind"]](spec))
            rec["solids"] = len(shape.solids())
            rec["valid"] = bool(shape.is_valid)
            rec["volume_mm3"] = gprops(shape)[0]
            rec["ok"] = rec["valid"] and rec["solids"] == 1 and rec["volume_mm3"] > 0.0
            if not rec["ok"]:
                raise RuntimeError("invalid solid (valid=%s, solids=%d)" % (rec["valid"], rec["solids"]))
            export_brep(shape, os.path.join(scratch, stem + ".brep"))
            built[stem] = shape
            if fm["step"]:
                p = os.path.join(out_dir, stem + ".step")
                export_step(shape, p)
                rec["files"].append(p)
            if fm["stl"]:
                p = os.path.join(out_dir, stem + ".stl")
                export_stl(shape, p, tolerance=0.02, angular_tolerance=0.2)
                rec["files"].append(p)
            if fm["dxf"] and spec.get("dxf"):
                p = os.path.join(out_dir, stem + ".dxf")
                if write_dxf(shape, spec["dxf"], p):
                    rec["files"].append(p)
        except Exception as e:
            rec["ok"] = False
            rec["error"] = str(e) or type(e).__name__
            rec["traceback"] = traceback.format_exc()
        rec["secs"] = time.time() - t0
        print(PART_TAG + json.dumps(rec), flush=True)
    asm = job.get("assembly")
    if asm:
        try:
            res = run_assembly(asm, built, scratch, out_dir)
        except Exception as e:
            res = {"ok": False, "status": "error", "part": asm["name"], "valid": False,
                   "volume_mm3": 0.0, "error": str(e), "traceback": traceback.format_exc()}
        print(RESULT_START + json.dumps(res) + RESULT_END, flush=True)
"#;
