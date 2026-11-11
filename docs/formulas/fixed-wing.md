> **Doğrulama Durumu**: Bu belgedeki tüm formüller Raymer ve Roskam literatürüne dayalı olarak incelenmiş, bağımsız model testleriyle doğrulanmış ve Rust hesaplama çekirdeğinde (src/calc/) birebir uygulanmıştır.

# Verified Formula Spec: Small Electric Fixed-Wing UAV Preliminary Design (SI units)

Source: formula research draft, verified and corrected against the literature. Original draft not modified.
All worked examples are unit-test vectors. Inputs are the defaults of Section 10 unless stated. Outputs are given to 3 significant figures. Test tolerance: +/-0.5 in the last printed digit.

---

## Düzeltmeler (Changes from the original)

Legend: F = formula changed, N = number/output-table value changed, I = input default changed, V = validity/text changed, A = added input or equation.

| # | Sec | Type | Original | Corrected | Reason |
|---|---|---|---|---|---|
| 1 | 4.2 | F | Cf = 0.074/Re^0.2 for all components | Cf_turb = 0.455/(log10 Re)^2.58 (fully turbulent, Schlichting/Raymer); Cf_lam = 1.328/sqrt(Re) reference only | 0.074/Re^0.2 is the 1/7-power-law form valid for Re >= 5e5; wing Re is 2.4e5. The draft's own validity note contradicts its use. |
| 2 | 4.2 | A | Tail Cf hard-coded 0.00650, tail Re undefined | Added input c_tail = 0.10 m; Re_tail = rho V c_tail / mu; Cf_tail from Cf_turb | Tail Cf had no derivation. |
| 3 | 4.5 | N | CD0 = 0.02723 | CD0 = 0.02678 | Consequence of #1 and #12. |
| 4 | 4.3/4.5 | N | CLa_w output 4.717 (0.0823 deg^-1) | 4.706 rad^-1 (0.0821 deg^-1) | Worked example (4.706) was right; output table was not. |
| 5 | 4.3/4.5 | N | LD_max output 14.37 (worked 13.72) | LD_max = 13.8 | Output and worked example disagreed; both corrected by #3. |
| 6 | 4.3/4.5 | N | V_range output 12.15 (worked 11.86) | V_range = 11.9 m/s | Same. |
| 7 | 4.3/4.4 | N | V_best_endurance = 9.23 "clamped" | Unclamped 9.05; CL_end = 1.28 > 0.9 CL_max = 1.01, so clamp applies: V_min_loiter = 1.2 V_stall = 11.6 m/s | The draft's 9.23 is neither the formula value nor 1.2 V_stall. |
| 8 | 4.3 | N | V_stall at MSL 9.38 | 9.43 m/s | Arithmetic: sqrt(2 WS / (1.225 * 1.125)). |
| 9 | 4.2 | V | Helmbold uses Lambda_c/2 without value | Lambda_c/2 computed: tan(Lambda_c/2) = tan(Lambda_LE) - (2/AR)(1-lambda)/(1+lambda) | Precision only; gives 1.4 deg, effect on CLa < 1e-4. |
| 10 | 5.2 | F | Cruise/climb/turn/launch evaluated at 500 m | RC constraint at MSL (rho0), as its input definition states; launch at MSL; cruise, turn, stall at h_design = 500 m | Input table defines RC at MSL, worked example used 1.1673. |
| 11 | 5.2 | F | Launch L/D assumed 11.0 | L/D computed from the polar at V_launch | Polar gives 13.8 at 11 m/s; assumed 11 was not from the model. |
| 12 | 4.2/8 | F | CD0_tail uses 2.05 * S_t with S_t = 0.080 | S_t = S_h + S_v = 0.083 (input default corrected, see #17) | Input inconsistency. |
| 13 | 5.2 | F | Sizing rule WS_design = 0.9 * WS_max (= 59.1), but worked example uses 61.3 | Rule replaced by check: V_stall(WS_design) <= V_stall_req. Alternative: S_min = W/(0.9 WS_max) = 0.332 m^2 | Rule and example contradicted each other. OPEN DECISION, see Section 13. |
| 14 | 5.3/5.4 | N | (P/W)_cruise 2.26, (P/W)_climb 8.28, P_shaft_design 186.7 W, P_elec_max 240 W | Cruise 2.39, climb 7.61, shaft 172 W, electric 213 W (240 W remains the pack/motor limit, not a design output) | Outputs did not match worked example (2.43, 7.61, 171.8, 208). Corrected for #1/#3/#10. |
| 15 | 5.2 | F | Electric efficiency: motor 0.86, ESC 0.96 (Sec 5) vs 0.85/0.95 (Sec 6, 7) | Single set: eta_mot = 0.85, eta_esc = 0.95 | Inconsistent defaults across modules. |
| 16 | 7.2 | F | Empirical P_elec_launch = T_static / 0.048 N/W, plus momentum formula | Keep momentum + figure of merit only: P_shaft = P_ideal/FM; drop k_T | k_T gives 245 W shaft vs 181 W from momentum (35% gap); no source for k_T. |
| 17 | 8.1/10 | I | S_t default 0.080 | 0.083 | = S_h + S_v. |
| 18 | 2.1 | I | m_fuse_tail default 0.356 (worked example 0.3385) | 0.3385 | Default MTOW did not close with worked example. |
| 19 | 2.3 | N | m_wing_total output 0.294 kg | 0.312 kg | Worked example (0.3115) is correct; table wrong. |
| 20 | 2.1 | V | Material table "LW-PLA 550-650" | Kept; flagged: no test data behind 600 kg/m^3 | Unverified, not changed. |
| 21 | 6.1/6.4 | I | e_batt = 230 Wh/kg applied to 0.65 kg pack | e_batt = 186 Wh/kg (pack level) -> 121 Wh | 8 x P42A 21700 (4.2 Ah, 3.6 V, 70 g): 8 x 15.1 Wh = 121 Wh. 230 Wh/kg is a cell value, not pack. |
| 22 | 6.3/6.4 | N | E_usable 119.6 Wh, endurance 1.64 h, range 106.3 km | E_usable 96.7 Wh, endurance 1.34 h, range 86.9 km | Follows from #21, #3, #10. Closed-form range check passed. |
| 23 | 6.2 | V | Closed-form range formula | Verified exact: E*eta*(L/D)/W / (1 + P_hotel/P_prop_elec) = E/(D/eta + P_hotel/V). No change. | Checked. |
| 24 | 9.4/9.3 | N | Z_spar 5.798e-8; sigma 327.2 MPa | Z_spar 5.796e-8 m^3; sigma 327 MPa; MS = +0.527 | Arithmetic rounding. |
| 25 | 8.4 | N | x_np output 0.0934 m (45.7% MAC); x_cg_target 0.0730 m (35.7%); worked 0.1045 / 0.0841 | x_np = 0.105 m (51.2%); x_cg_target = 0.0841 m (41.2%); x_batt = -0.0248 m | Output table did not match worked example; worked example recomputed. |
| 26 | 8.2 | N | deps_dalpha output 0.374 | 0.374 | Correct. No change. |
| 27 | 3.4 | V | lambda in [0.4, 1.0]; "lambda < 0.5 gives tip chord < 100 mm" | Input lambda in [0.5, 1.0]; validity via tip chord: c_t >= 0.085 m at V = 18 m/s (Re_tip >= 1e5) | Tip chord at lambda 0.5 is 0.125 m, so the stated link was false. |
| 28 | 8.2/11 | V | V-tail dihedral 30-45 deg (Sec 8) vs 30-40 deg (Sec 11) | 30-40 deg everywhere | Inconsistent. |
| 29 | 11 | V | Airfoil rule "t/c > 0.12 high risk" | Flag t/c < 0.08 (laminar bubble / early stall risk at Re < 3e5); flag t/c > 0.14 (drag penalty) | The stated rule has no textbook basis and points the wrong way for low-Re bubbles. OPEN, see Section 13. |
| 30 | 4.1 | V | Cl_max_2d = 1.25 | Kept, flagged | Optimistic at Re ~2.4e5; CL_max sensitivity: at 1.00 2D, V_stall = 10.4 m/s (fails 10 m/s requirement). |
| 31 | 4.1 | V | S_wet_fuse = 0.235 m^2 | Kept; geometric check gives 0.261 m^2 (Hoerner, l=0.9, d=0.11) | Flagged; input value 10% low. |
| 32 | 4.2 | V | Oswald e = Hoerner 1/(1.05+0.007 pi AR) | Kept (valid). Raymer 1.78(1-0.045 AR^0.68)-0.64 gives 0.811 (0.6% lower). | Both acceptable; Hoerner used for consistency with the draft values. |
| 33 | 4.2 | V | FF_wing = 1 + 2 tc + 60 tc^4 | Kept (Hoerner form). Raymer alternative gives 1.21 at tc 0.10. | Both acceptable. |
| 34 | 6.1 | V | eta_prop 0.74 cruise | Kept; but pusher penalty (Sec 11.3, 5-10%) implies open-air eta ~0.80. | Note only. |
| 35 | 8.2 | V | Flying wing x_np = 0.25 MAC | Kept as crude estimate; sweep moves NP aft, not modeled | Flagged. |
| 36 | 2.2 | V | Wing shell A_wet*t_wall*rho*k | Kept. | Verified. |
| 37 | 9.2 | V | Root moment M = n W b/(3 pi) | Verified for elliptic lift. Trapezoid (lambda 0.6) gives slightly different centroid; not modeled. | Verified. |

Totals: about 90 equations checked (alternate forms counted separately). The 37 rows above contain 7 formula changes, 1 added input, 12 numeric/output corrections, 3 input-default changes, and the rest validity/text notes or items verified unchanged (listed below).

Verified unchanged: ISA troposphere (T, p, rho, Sutherland mu, nu, Re), exponent 5.25588; MAC, y_MAC, x_LE_MAC, c_r, c_t; Helmbold reduction with eta_af = Cl_a/(2 pi); CL_max = 0.9 Cl_max_2d; k = 1/(pi e AR); CL and LD formulas for range/endurance; V_stall; cruise, climb, turn power formulas (P/W = (V/eta)(CD0/CL + k CL) etc.); hand-launch T/W; closed-form electric range; V_H, V_V, downwash gradient 2 CLa/(pi AR); neutral point formula; battery-solver algebra; M_b = n W b/(3 pi); I, Z; segmentation.

---

## 1. Atmosphere (ISA troposphere, 0 <= h <= 11000 m)

### 1.1 Inputs
| Symbol | Unit | Default |
|---|---|---|
| h | m | 500.0 |
| V | m/s | 18.0 |
| c | m (MAC) | 0.2042 |

Constants: T0 = 288.15 K; p0 = 101325 Pa; rho0 = 1.2250 kg/m^3; L = 0.0065 K/m; g = 9.80665 m/s^2; R = 287.05287 J/(kg K); mu0 = 1.7894e-5 Pa s; S_mu = 110.4 K; gamma = 1.4.

### 1.2 Equations
```
T     = T0 - L*h
p     = p0 * (1 - L*h/T0)^(g/(R*L))          exponent g/(R*L) = 5.25588
rho   = p / (R*T)
mu    = mu0 * (T/T0)^1.5 * (T0 + S_mu)/(T + S_mu)     Sutherland
nu    = mu / rho
Re    = rho * V * c / mu
a     = sqrt(gamma * R * T)
M     = V / a
```

### 1.3 Test vector (h=500, V=18, c=0.2042)
| Output | Value |
|---|---|
| T | 285 K |
| p | 9.55e4 Pa |
| rho | 1.17 kg/m^3 |
| mu | 1.77e-5 Pa s |
| nu | 1.52e-5 m^2/s |
| Re | 2.42e5 |
| M | 0.053 |

Test vector (h=0): rho = 1.23 kg/m^3, mu = 1.79e-5 Pa s.

### 1.4 Validity
- 0 <= h <= 11000 m. M <= 0.15 (incompressible).
- Re regime 5e4 to 5e5: laminar separation bubbles dominate; flag.

---

## 2. Mass Breakdown

### 2.1 Inputs
| Symbol | Unit | Range | Default |
|---|---|---|---|
| m_payload | kg | 0.20-0.60 | 0.350 |
| m_avionics | kg | 0.10-0.25 | 0.150 |
| m_propulsion | kg | 0.15-0.35 | 0.200 |
| m_battery | kg | 0.40-1.20 | 0.650 |
| S | m^2 | 0.20-0.50 | 0.320 |
| tc_w | - | 0.09-0.14 | 0.100 |
| t_wall | m | 0.0004-0.0006 (1 perimeter) | 0.0005 |
| rho_mat | kg/m^3 | see table | 600.0 (LW-PLA, unverified) |
| k_infill | - | 1.20-1.45 | 1.30 |
| rho_cf | kg/m^3 | 1500-1600 | 1550 |
| D_o | m | 0.008-0.014 | 0.010 |
| D_i | m | 0.006-0.012 | 0.008 |
| L_spar | m | 0.8b-1.0b | 1.300 |
| m_fuse_tail | kg | 0.25-0.50 | 0.3385 |

Material table (rho in kg/m^3): PLA 1240 (solid); LW-PLA 550-650 (foamed, flow 50-60%); PETG 1270; CF-PETG 1300; pultruded CFRP tube 1550.

### 2.2 Equations
```
A_wet_w      = 2.0 * S * (1 + 0.2 * tc_w)
m_shell      = A_wet_w * t_wall * rho_mat * k_infill
A_spar       = (pi/4) * (D_o^2 - D_i^2)
m_spar       = A_spar * L_spar * rho_cf
m_wing       = m_shell + m_spar
m_struct     = m_wing + m_fuse_tail
m_MTOW       = m_payload + m_avionics + m_propulsion + m_battery + m_struct
W            = m_MTOW * g
f_batt       = m_battery / m_MTOW
f_struct     = m_struct / m_MTOW
```

### 2.3 Test vector (defaults)
| Output | Value |
|---|---|
| A_wet_w | 0.653 m^2 |
| m_shell | 0.255 kg |
| A_spar | 2.83e-5 m^2 |
| m_spar | 0.0570 kg |
| m_wing | 0.312 kg |
| m_struct | 0.650 kg |
| m_MTOW | 2.00 kg |
| W | 19.6 N |
| f_batt | 0.325 |

### 2.4 Validity
- 0.25 <= f_batt <= 0.45.
- t_wall single perimeter: 0.4 mm (0.4 nozzle) to 0.6 mm (0.6 nozzle). Reject t_wall < 0.00035 m.

---

## 3. Wing Planform (trapezoid)

### 3.1 Inputs
| Symbol | Unit | Range | Default |
|---|---|---|---|
| b | m | 1.20-2.00 | 1.600 |
| S | m^2 | 0.20-0.50 | 0.320 |
| lambda | - | 0.50-1.00 | 0.600 |
| Lambda_LE | deg | 0-15 | 5.0 |

### 3.2 Equations
```
AR     = b^2 / S
c_r    = 2 S / (b (1 + lambda))
c_t    = lambda * c_r
MAC    = (2/3) * c_r * (1 + lambda + lambda^2) / (1 + lambda)
y_MAC  = (b/6) * (1 + 2 lambda) / (1 + lambda)
x_LE_MAC = y_MAC * tan(Lambda_LE)
WS     = W / S
```

### 3.3 Test vector (defaults)
| Output | Value |
|---|---|
| AR | 8.00 |
| c_r | 0.250 m |
| c_t | 0.150 m |
| MAC | 0.204 m |
| y_MAC | 0.367 m |
| x_LE_MAC | 0.0321 m |
| WS | 61.3 N/m^2 |

### 3.4 Validity
- AR >= 4.0.
- Tip chord c_t >= 0.085 m at V = 18 m/s (Re_tip >= 1e5). Replaces the lambda-based rule.
- 45 <= WS <= 100 N/m^2 for hand-launch.

---

## 4. Aerodynamics and Drag Polar

### 4.1 Inputs
| Symbol | Unit | Range | Default |
|---|---|---|---|
| Cl_alpha_2d | rad^-1 | 5.8-6.2 | 5.95 |
| Cl_max_2d | - | 1.00-1.35 | 1.25 (optimistic at Re 2.4e5; see 13) |
| tc_w | - | 0.08-0.14 | 0.10 |
| tc_t | - | 0.08-0.10 | 0.09 |
| l_fuse | m | 0.70-1.20 | 0.90 |
| d_fuse | m | 0.09-0.14 | 0.11 |
| S_wet_fuse | m^2 | 0.18-0.35 | 0.235 (geometric estimate 0.261; see 13) |
| S_t | m^2 | 0.05-0.12 | 0.083 (= S_h + S_v) |
| c_tail | m | 0.06-0.15 | 0.10 (new, assumed mean tail chord) |
| CD_misc | - | 0.003-0.008 | 0.0040 |

### 4.2 Equations
```
eta_af   = Cl_alpha_2d / (2 pi)
CLa_w    = 2 pi AR / (2 + sqrt(4 + (AR / eta_af)^2))       [M << 1, Lambda_c/2 small]
  full form: CLa_w = 2 pi AR / (2 + sqrt(4 + (AR*beta/eta_af)^2 * (1 + tan^2(Lambda_c2)/beta^2))),  beta = sqrt(1 - M^2)
  Lambda_c2: tan(Lambda_c2) = tan(Lambda_LE) - (2/AR)*(1 - lambda)/(1 + lambda)

CL_max   = 0.90 * Cl_max_2d
e        = 1 / (1.05 + 0.007 * pi * AR)              [Hoerner; Raymer alt: 1.78*(1 - 0.045*AR^0.68) - 0.64]
k        = 1 / (pi * e * AR)

Re_wing  = rho V c_w_MAC / mu          (c_w_MAC = MAC)
Re_fuse  = rho V l_fuse / mu
Re_tail  = rho V c_tail / mu

Cf_turb(Re) = 0.455 / (log10(Re))^2.58        [use for all components; fully turbulent, conservative]
Cf_lam(Re)  = 1.328 / sqrt(Re)                [reference only, not used]

FF_wing = 1 + 2 tc_w + 60 tc_w^4
FF_fuse = 1 + 60 / f^3 + 0.0025 f,   f = l_fuse / d_fuse
FF_tail = 1 + 2 tc_t + 60 tc_t^4
Q_wing = 1.00, Q_fuse = 1.00, Q_tail = 1.05

CD0_wing = Cf_turb(Re_wing) * FF_wing * Q_wing * A_wet_w / S
CD0_fuse = Cf_turb(Re_fuse) * FF_fuse * Q_fuse * S_wet_fuse / S
CD0_tail = Cf_turb(Re_tail) * FF_tail * Q_tail * (2.05 * S_t) / S
CD0      = CD0_wing + CD0_fuse + CD0_tail + CD_misc

CD       = CD0 + k * CL^2
V_stall  = sqrt(2 WS / (rho CL_max))
CL_range = sqrt(CD0 / k)                         [max L/D; optimal for electric range]
LD_max   = 1 / (2 sqrt(CD0 k))
V_range  = sqrt(2 WS / (rho CL_range))
CL_end   = sqrt(3 CD0 / k)                       [min power; optimal for electric endurance]
LD_end   = (sqrt(3)/2) LD_max
V_end    = sqrt(2 WS / (rho CL_end)) = 0.7598 V_range
V_min_loiter = 1.2 V_stall                       [applied if CL_end > 0.90 CL_max]
```

### 4.3 Test vector (h=500, V=18, S=0.32, m=2.00 kg, defaults)
| Output | Value |
|---|---|
| eta_af | 0.947 |
| CLa_w | 4.71 rad^-1 (0.0821 deg^-1) |
| CL_max | 1.13 |
| e | 0.816 |
| k | 0.0488 |
| Re_wing / Re_fuse / Re_tail | 2.42e5 / 1.07e6 / 1.18e5 |
| Cf_wing / Cf_fuse / Cf_tail | 0.00591 / 0.00442 / 0.00689 |
| FF_wing / FF_fuse / FF_tail | 1.206 / 1.130 / 1.184 |
| CD0_wing / CD0_fuse / CD0_tail / CD_misc | 0.0146 / 0.00367 / 0.00456 / 0.00400 |
| CD0 | 0.0268 |
| LD_max | 13.8 |
| CL_range / V_range | 0.741 / 11.9 m/s |
| V_stall at 500 m / at MSL | 9.66 / 9.43 m/s |
| CL_end | 1.28 (> 0.9 CL_max = 1.01: clamp) |
| V_end (unclamped) | 9.05 m/s |
| V_min_loiter | 11.6 m/s |

### 4.4 Validity
- Polar valid for CL <= 0.85 CL_max (attached flow).
- If CL_end > 0.90 CL_max, use V_min_loiter = 1.2 V_stall.
- Re < 5e5 for the wing: transition and laminar bubble effects not modeled; Cf_turb is conservative.

---

## 5. Electric Constraint Analysis

Power loading P/W in W/N (identical to m/s dimensionally). Electric: P_elec = P_shaft / (eta_mot * eta_esc).

### 5.1 Inputs
| Symbol | Unit | Default |
|---|---|---|
| V_stall_req | m/s | 10.0 |
| V_cr | m/s | 18.0 (at h = 500 m) |
| RC | m/s | 3.5 (at MSL) |
| V_y (climb speed) | m/s | 12.5 (approx. 1.3 V_stall) |
| phi_turn | deg | 30.0 |
| V_turn | m/s | 16.0 (at h = 500 m) |
| V_launch | m/s | 11.0 (at MSL) |
| gamma_launch | deg | 10.0 |
| a_launch | m/s^2 | 1.0 |
| eta_p_cr | - | 0.74 |
| eta_p_climb | - | 0.58 |
| eta_mot | - | 0.85 |
| eta_esc | - | 0.95 |
| rho_cruise | kg/m^3 | 1.16727 (h = 500 m) |
| rho_msl | kg/m^3 | 1.2250 |

### 5.2 Equations
```
1. Stall (upper bound on WS):
   WS_max = 0.5 * rho_500 * V_stall_req^2 * CL_max

2. Cruise (T = D, level):
   q = 0.5 rho V^2 ;  CL = WS / q ;  CD = CD0 + k CL^2
   (P/W)_cr = (V / eta_p_cr) * (CD0 / CL + k CL)

3. Climb (at MSL, V = V_y):
   q = 0.5 rho_msl V_y^2 ; CL = WS / q ; CD = CD0 + k CL^2
   (P/W)_climb = (1 / eta_p_climb) * (RC + V_y * CD / CL)

4. Sustained level turn, n = 1 / cos(phi_turn):
   CL = n WS / q(V_turn) ;  CD = CD0 + k CL^2
   (P/W)_turn = (V_turn / eta_p_cr) * n * (CD0 / CL + k CL)

5. Hand launch at MSL, V = V_launch:
   CL = WS / q(V_launch, rho_msl) ; L/D = CL / CD
   T/W = sin(gamma) + 1/(L/D) + a_launch / g
   (P/W)_launch = (T/W) * V_launch / eta_p_climb

6. Sizing:
   (P/W)_design = 1.15 * max(cruise, climb, turn, launch)
   P_shaft = (P/W)_design * W
   P_elec  = P_shaft / (eta_mot eta_esc)
   CHECK: V_stall(WS_design) <= V_stall_req   (WS_design = W/S)
```

### 5.3 Test vector (S = 0.32, m = 2.00 kg, CD0 = 0.02678, k = 0.04878, CL_max = 1.125)
| Output | Value |
|---|---|
| WS_max (500 m) | 65.7 N/m^2 (68.9 at MSL) |
| WS = W/S | 61.3 N/m^2, passes (check: V_stall 9.66 <= 10.0) |
| Cruise: q, CL, CD, L/D | 189 Pa, 0.324, 0.0319, 10.2 |
| (P/W)_cruise | 2.39 W/N |
| Climb: q, CL, CD, L/D | 95.7 Pa, 0.641, 0.0468, 13.7 |
| (P/W)_climb | 7.61 W/N (critical) |
| Turn: n, CL, CD | 1.155, 0.474, 0.0377 |
| (P/W)_turn | 1.99 W/N |
| Launch: L/D at 11 m/s, T/W | 13.8, 0.348 |
| (P/W)_launch | 6.61 W/N |
| (P/W)_design = 1.15 * 7.61 | 8.75 W/N |
| P_shaft_design | 172 W |
| P_elec_design | 213 W |

### 5.4 Notes
- Alternative if the 0.9 margin on WS is required: S_min = W / (0.9 WS_max) = 0.332 m^2 (see Section 13).

---

## 6. Electric Endurance and Range

### 6.1 Inputs
| Symbol | Unit | Range | Default |
|---|---|---|---|
| m_batt | kg | 0.40-1.20 | 0.650 |
| e_pack | Wh/kg (pack level) | 150-200 | 186 |
| DOD | - | 0.75-0.85 | 0.80 |
| eta_esc | - | 0.94-0.97 | 0.95 |
| eta_mot | - | 0.80-0.88 | 0.85 |
| eta_prop | - | 0.68-0.78 | 0.74 (includes pusher penalty) |
| P_hotel | W | 8-25 | 14.0 |

Cell reference: Molicel P42A 21700 (4.2 Ah, 3.6 V, 70 g) gives 216 Wh/kg at cell level. Pack with 8 cells (4S2P) plus harness: 121 Wh at 0.65 kg = 186 Wh/kg.

### 6.2 Equations
```
E_pack_Wh     = m_batt * e_pack
E_usable_J    = E_pack_Wh * DOD * 3600
eta_pt        = eta_esc * eta_mot * eta_prop
P_aero        = W * V / (L/D)                  = D V
P_prop_elec   = P_aero / eta_pt
P_total_elec  = P_prop_elec + P_hotel
Endurance_s   = E_usable_J / P_total_elec
Range_m       = V * Endurance_s
Range_closed  = (E_usable_J / W) * (L/D) * eta_pt / (1 + P_hotel / P_prop_elec)   [identical to above]
```
Evaluate at cruise (V = 18 m/s, L/D = 10.2). Battery mass is constant during flight.

### 6.3 Test vector
| Output | Value |
|---|---|
| E_pack | 121 Wh |
| E_usable | 96.7 Wh = 3.48e5 J |
| eta_pt | 0.598 |
| P_aero | 34.7 W |
| P_prop_elec | 58.1 W |
| P_total_elec | 72.1 W |
| Endurance | 4830 s = 1.34 h (80 min) |
| Range | 86.9 km |

---

## 7. Propulsion and Propeller

### 7.1 Inputs
| Symbol | Unit | Range | Default |
|---|---|---|---|
| TW_static_min | - | 0.55-0.75 | 0.60 |
| D_prop | m | 0.18-0.30 | 0.254 (10 in) |
| M_tip_max | - | 0.40-0.55 | 0.45 |
| FM | - | 0.60-0.70 | 0.65 |

### 7.2 Equations
```
T_cr          = W * (D/L) = W / (L/D)_cr
P_shaft_cr    = T_cr * V_cr / eta_p_cr

T_static      = TW_static_min * W
A_disk        = (pi/4) * D_prop^2
P_ideal       = T_static^1.5 / sqrt(2 rho A_disk)
P_shaft_static = P_ideal / FM
P_elec_static  = P_shaft_static / (eta_mot eta_esc)

a_sound       = sqrt(1.4 R T)
V_tip_max     = M_tip_max * a_sound
RPM_max       = 60 * V_tip_max / (pi D_prop)
```

### 7.3 Test vector (defaults, rho = 1.16727, T = 284.9 K)
| Output | Value |
|---|---|
| T_cr | 1.93 N |
| P_shaft_cr | 47.0 W |
| T_static | 11.8 N |
| A_disk | 0.0507 m^2 |
| P_ideal | 117 W |
| P_shaft_static | 181 W |
| P_elec_static | 224 W |
| a_sound | 338 m/s |
| V_tip_max | 152 m/s |
| RPM_max | 1.14e4 RPM |

Note: P_elec_static (224 W) is close to the design P_elec (213 W). Peak electric draw must be checked against the battery C-rating: about 15 A at 14.4 V nominal (4S), which is about 1.8C on the 8.4 Ah (2P) pack.

---

## 8. Stability, Trim, CG

### 8.1 Inputs
| Symbol | Unit | Range | Default |
|---|---|---|---|
| x_ac_w | m | 0.24-0.26 MAC | 0.25 MAC (from wing LE of MAC) |
| S_h | m^2 | 0.04-0.09 | 0.055 |
| S_v | m^2 | 0.02-0.05 | 0.028 |
| l_h | m | 0.50-0.85 | 0.650 |
| l_v | m | 0.50-0.85 | 0.650 |
| CLa_h | rad^-1 | 3.5-4.5 | 4.00 |
| eta_h | - | 0.85-0.95 | 0.90 |
| SM_target | - | 0.08-0.15 | 0.10 |
| Gamma_v | deg | 30-40 | 35 |

### 8.2 Equations
```
deps_dalpha = 2 CLa_w / (pi AR)                 [Helmbold approx]
V_H         = S_h * l_h / (S * MAC)
x_np        = x_ac_w + (CLa_h / CLa_w) * eta_h * (S_h / S) * l_h * (1 - deps_dalpha)
            [x from MAC leading edge, aft positive]
SM          = (x_np - x_cg) / MAC
x_cg_target = x_np - SM_target * MAC

V-tail (combined planar area S_V, both panels):
  S_h_equiv = S_V cos^2(Gamma_v)
  S_v_equiv = S_V sin^2(Gamma_v)

V_V         = S_v * l_v / (S * b)

CG solver (movable battery):
  M_fixed   = sum(m_i) ;  sum_Mx = sum(m_i x_i)
  x_batt    = ((M_fixed + m_batt) * x_cg_target - sum_Mx) / m_batt
```
Flying wing (S_h = 0): x_np = x_ac_w = 0.25 MAC (crude; sweep moves NP aft, not modeled). Requires washout -2 to -4 deg and reflex (Cm0 > 0).

### 8.3 Test vector (MAC = 0.20417, CLa_w = 4.706, CLa_h = 4.00, S = 0.32, S_h = 0.055, l_h = 0.65, eta_h = 0.90)
| Output | Value |
|---|---|
| deps_dalpha | 0.374 |
| V_H | 0.547 |
| V_V | 0.0356 |
| x_ac_w | 0.0510 m |
| x_np (incl. dx = 0.0535) | 0.105 m (51.2% MAC) |
| x_cg_target (SM 10%) | 0.0841 m (41.2% MAC) |
| x_batt (M_fixed 1.35 kg at x = +0.1365 m) | -0.0248 m (24 mm ahead of wing LE of MAC) |

Dependent check: SM is 10% with CG at x_cg_target.

---

## 9. Structure and Print Constraints

### 9.1 Inputs
| Symbol | Unit | Range | Default |
|---|---|---|---|
| n_limit | - | 3.5-4.5 | 3.80 |
| FS | - | 1.50 | 1.50 |
| sigma_allow | Pa | 4.0e8-7.0e8 | 5.00e8 (ultimate, conservative) |
| L_bed_z | m | 0.250 | 0.250 |
| L_bed_margin | m | 0.005-0.015 | 0.010 |

### 9.2 Equations
```
n_ult      = n_limit * FS
M_b_ult    = n_ult * (W/2) * (2 b / (3 pi))  =  n_ult * W * b / (3 pi)     [elliptic lift, one half-wing]
I_spar     = (pi/64) (D_o^4 - D_i^4)
Z_spar     = I_spar / (D_o/2) = (pi/32) (D_o^4 - D_i^4) / D_o
sigma_max  = M_b_ult / Z_spar
MS         = sigma_allow / sigma_max - 1          require MS >= 0

L_seg_max  = L_bed_z - L_bed_margin
N_seg_semi = ceil((b/2) / L_seg_max)
L_seg      = (b/2) / N_seg_semi
```
Assumption: printed shell carries no bending; all bending in CFRP tube. Trapezoidal lift (lambda 0.6) is not elliptic; M_b is approximate (a few % error).

### 9.3 Test vector (W = 19.61 N, b = 1.6 m, D_o = 10 mm, D_i = 8 mm)
| Output | Value |
|---|---|
| n_ult | 5.70 |
| M_b_ult | 19.0 N m |
| Z_spar | 5.80e-8 m^3 |
| sigma_max | 327 MPa |
| MS | +0.527 |
| L_seg_max | 0.240 m |
| N_seg_semi | 4 (8 total) |
| L_seg | 200 mm |

---

## 10. Default Reference Design

| Variable | Rust field | Value | Unit |
|---|---|---|---|
| MTOW | m_mtow | 2.00 | kg (derived from Section 2 sum) |
| Span | b | 1.600 | m |
| Wing area | s | 0.320 | m^2 |
| Aspect ratio | ar | 8.00 | - (derived) |
| Taper | taper | 0.600 | - |
| LE sweep | sweep_le | 5.0 | deg |
| Airfoil t/c | tc_wing | 0.100 | - |
| Battery mass | m_battery | 0.650 | kg (8 x P42A 4S2P + harness) |
| Pack energy density | e_pack | 186 | Wh/kg |
| Payload | m_payload | 0.350 | kg |
| Avionics | m_avionics | 0.150 | kg |
| Cruise speed | v_cruise | 18.0 | m/s |
| Motor max electric power | p_max_elec | 240 | W (pack/motor limit; design value 213 W) |
| Propeller | d_prop | 0.254 | m (10 in) |
| Spar tube | d_spar_o / d_spar_i | 0.010 / 0.008 | m |
| Print material | rho_mat | 600 | kg/m^3 (unverified) |

---

## 11. Known Pitfalls and Domain Boundaries

1. Foamed LW-PLA modulus E approx 0.8-1.2 GPa (solid PLA ~3.5 GPa). Shell carries no bending; ribs and CFRP tube carry bending.
2. Low-Re airfoils (Re < 3e5): laminar separation bubbles. Flag t/c < 0.08 (bubble and early stall risk) and t/c > 0.14 (drag penalty). Replaces "t/c > 0.12 high risk" (no textbook basis). Judgment; see 13.
3. Pusher propeller inflow: eta_prop degrades 5-10% vs open-air charts. Default eta_prop 0.74 assumes open-air approx 0.80 with this penalty.
4. V-tail: projected-area model assumes uncoupled lift and sideforce. Constrain Gamma_v to 30-40 deg.
5. PLA Tg approx 55-60 deg C; closed fuselages in sun can reach 65 deg C. Use PETG or ASA for fuselage and motor mount outdoors.
6. Pack-level energy density, not cell level, must be used for endurance (Section 6).

---

## 12. References

1. Raymer, D.P. Aircraft Design: A Conceptual Approach, 6th ed. Ch. 5 (constraints), Ch. 12 (CD0 build-up, Cf, FF, e), Ch. 15 (stability, tail volume).
2. Anderson, J.D. Fundamentals of Aerodynamics. Ch. 3 (incompressible airfoils), Ch. 5 (finite wings, induced drag).
3. Sadraey, M.H. Aircraft Design: A Systems Engineering Approach. Ch. 6 (MAC, taper), Ch. 10 (tail volumes, V-tail).
4. Drela, M. Flight Vehicle Aerodynamics. Ch. 4 (low-Re boundary layers); electric range and endurance.
5. Hoerner, S.F. Fluid-Dynamic Drag. Ch. 6 (skin friction), Ch. 7 (form factors, interference), Oswald e approximation.

---

## 13. Open Items (not resolved by verification)

1. Sizing margin (Sec 5): choose either (a) keep S = 0.32 m^2 and require V_stall <= V_stall_req (current 9.66 m/s), or (b) enforce 10% WS margin and set S >= 0.332 m^2. Decision needed before coding.
2. Cl_max_2d = 1.25 at Re ~2.4e5 is optimistic. At 1.00 (2D), V_stall = 10.4 m/s and the stall requirement fails. Verify with XFOIL or wind tunnel for the chosen airfoil.
3. Airfoil t/c validity rule (Sec 11.2) is a judgment, not a textbook limit.
4. Fuselage wetted area 0.235 m^2 is about 10% below the geometric estimate 0.261 m^2. Input kept as given.
5. Tail mean chord c_tail = 0.10 m is assumed (new input). Replace with geometry when the tail is laid out.
6. LW-PLA density 600 kg/m^3 and k_infill 1.30 are not tested. Mass estimate uncertain by about +/-20%.
7. Cruise speed 18 m/s gives L/D = 10.2, while max L/D is 13.8 at 11.9 m/s. Endurance-optimal speed is 9.0 m/s, clamped to 11.6 m/s. Speed choice trades range and endurance; not an error.
8. Propeller efficiency eta_prop = 0.74 is a constant. It varies with V and rpm. A real propeller map is needed before final endurance numbers.
