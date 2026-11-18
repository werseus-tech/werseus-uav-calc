// SPDX-License-Identifier: AGPL-3.0-or-later
//! Elle yazılan DXF R12 (AC1009) ASCII çıktısı: kapalı POLYLINE kesim yolları (lazer/CNC),
//! birim mm. Okuma tarafı yalnızca bu yazıcının çıktısını geri çözer (testler ve doğrulama).

use std::fmt::Write;

/// Kapalı çokgen çizgileri tek katmanda ("CUT") R12 DXF metnine çevirir.
pub fn write_dxf(loops: &[Vec<[f64; 2]>], layer: &str) -> String {
    let mut s = String::new();
    let mut g = |code: i32, v: &str| {
        let _ = write!(s, "{code}\n{v}\n");
    };
    g(0, "SECTION");
    g(2, "HEADER");
    g(9, "$ACADVER");
    g(1, "AC1009");
    g(9, "$INSUNITS");
    g(70, "4");
    g(0, "ENDSEC");
    g(0, "SECTION");
    g(2, "TABLES");
    g(0, "TABLE");
    g(2, "LTYPE");
    g(70, "1");
    g(0, "LTYPE");
    g(2, "CONTINUOUS");
    g(70, "0");
    g(3, "Solid line");
    g(72, "65");
    g(73, "0");
    g(40, "0.0");
    g(0, "ENDTAB");
    g(0, "TABLE");
    g(2, "LAYER");
    g(70, "1");
    g(0, "LAYER");
    g(2, layer);
    g(70, "0");
    g(62, "7");
    g(6, "CONTINUOUS");
    g(0, "ENDTAB");
    g(0, "ENDSEC");
    g(0, "SECTION");
    g(2, "ENTITIES");
    for lp in loops {
        if lp.len() < 3 {
            continue;
        }
        g(0, "POLYLINE");
        g(8, layer);
        g(66, "1");
        g(70, "1");
        for p in lp {
            g(0, "VERTEX");
            g(8, layer);
            g(10, &format!("{:.4}", p[0]));
            g(20, &format!("{:.4}", p[1]));
            g(30, "0.0");
        }
        g(0, "SEQEND");
        g(8, layer);
    }
    g(0, "ENDSEC");
    g(0, "EOF");
    s
}

/// Okunan çokgen çizgi: (noktalar, kapalı mı).
pub type ReadPolyline = (Vec<[f64; 2]>, bool);

/// Geri okuma.
pub fn read_polylines(dxf: &str) -> Result<Vec<ReadPolyline>, String> {
    let lines: Vec<&str> = dxf.lines().map(str::trim).collect();
    if !lines.len().is_multiple_of(2) {
        return Err("odd number of lines".into());
    }
    let pairs: Vec<(i32, &str)> = lines
        .chunks(2)
        .map(|c| c[0].parse::<i32>().map(|k| (k, c[1])))
        .collect::<Result<_, _>>()
        .map_err(|e| e.to_string())?;
    let mut out = Vec::new();
    let mut cur: Option<ReadPolyline> = None;
    let (mut x, mut y) = (0.0, 0.0);
    let mut in_vertex = false;
    let mut have_eof = false;
    for (code, v) in pairs {
        if code == 0 {
            if in_vertex {
                if let Some(c) = cur.as_mut() {
                    c.0.push([x, y]);
                }
                in_vertex = false;
            }
            match v {
                "POLYLINE" => cur = Some((Vec::new(), false)),
                "VERTEX" => in_vertex = true,
                "SEQEND" => {
                    if let Some(c) = cur.take() {
                        out.push(c);
                    }
                }
                "EOF" => have_eof = true,
                _ => {}
            }
        } else if cur.is_some() {
            match code {
                70 if !in_vertex => {
                    let f: i32 = v.parse().map_err(|_| "bad flag")?;
                    if let Some(c) = cur.as_mut() {
                        c.1 = f & 1 == 1;
                    }
                }
                10 if in_vertex => x = v.parse().map_err(|_| "bad x")?,
                20 if in_vertex => y = v.parse().map_err(|_| "bad y")?,
                _ => {}
            }
        }
    }
    if !have_eof {
        return Err("missing EOF".into());
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_closed_polylines() {
        let a = vec![[0.0, 0.0], [10.0, 0.0], [10.0, 5.0], [0.0, 5.0]];
        let b = vec![[2.0, 1.0], [3.5, 1.0], [3.0, 2.25]];
        let txt = write_dxf(&[a.clone(), b.clone()], "CUT");
        assert!(txt.contains("AC1009") && txt.trim_end().ends_with("EOF"));
        let back = read_polylines(&txt).unwrap();
        assert_eq!(back.len(), 2);
        for ((pts, closed), src) in back.iter().zip([&a, &b]) {
            assert!(*closed);
            assert_eq!(pts.len(), src.len());
            for (p, q) in pts.iter().zip(src.iter()) {
                assert!((p[0] - q[0]).abs() < 1e-4 && (p[1] - q[1]).abs() < 1e-4);
            }
        }
    }
}
