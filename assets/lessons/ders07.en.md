# Lesson 7: Aerodynamic Stall: Causes, Prevention, and Hand-Launch Speed

### What you will learn
- How boundary layer flow separation and exceeding the critical angle of attack ($\alpha_{crit}$) cause an aerodynamic stall in fixed-wing aircraft.
- The mathematical derivation of level-flight stall speed ($V_{stall}$) as a function of wing loading ($W/S$) and maximum lift coefficient ($C_{L,max}$).
- Why safe hand launching requires an airspeed of $V_{launch} \ge 1.1 - 1.2 V_{stall}$ and how launch dynamics prevent immediate stall.

### Explanation
Imagine holding the back of a soup spoon under a steady stream of tap water. The water clings smoothly to the curved contour of the spoon, pulling the spoon into the stream. But if you tilt the spoon at an abrupt, steep angle relative to the flow, the water can no longer follow the surface; it separates violently, splashing outward in turbulent droplets. The airflow passing over an aircraft wing behaves in precisely the same manner.

At moderate angles of attack, the upper curvature of the airfoil accelerates oncoming air, creating suction that generates lift. Air molecules flow smoothly in a thin, cohesive boundary layer. However, as the pilot pulls the nose up to increase the angle of attack ($\alpha$), the airflow must fight uphill against an unfavorable adverse pressure gradient toward the trailing edge. When the angle of attack exceeds the **Critical Angle of Attack ($\alpha_{crit}$, typically $12^\circ - 15^\circ$)**, the boundary layer runs out of kinetic energy. The flow detaches completely from the upper wing surface, creating a chaotic, recirculating wake.

The moment flow separation occurs, aerodynamic lift collapses abruptly, while pressure drag rises sharply. The wing can no longer support the weight of the aircraft, and the airplane begins to drop out of the sky. This aerodynamic phenomenon is called a **Stall**. The minimum speed at which the wing producing maximum lift ($C_{L,max}$) can sustain level flight against gravity is the **Stall Speed ($V_{stall}$)**:
$$V_{stall} = \sqrt{\frac{2 (W/S)}{\rho C_{L,max}}}$$

Here, $W/S$ is the wing loading ($\text{N/m}^2$) and $\rho$ is air density. For our verified $2.0\text{ kg}$ reference UAV ($W \approx 19.6\text{ N}$) with a wing area of $0.32\text{ m}^2$ and $C_{L,max} = 1.125$ at sea level ($\rho = 1.225\text{ kg/m}^3$), the stall speed is $V_{stall} = 9.43\text{ m/s}$ (about $34\text{ km/h}$). Below this speed, unaccelerated level flight is aerodynamically impossible.

This physics becomes critical during a **Hand Launch**. When an operator launches a fixed-wing UAV by hand, human throwing speed is typically limited to $6 - 8\text{ m/s}$. If the UAV's stall speed is $9.4\text{ m/s}$ and the motor cannot accelerate the craft rapidly, the wings cannot generate sufficient lift. If an inexperienced pilot reacts by pulling up on the elevator stick to prevent the aircraft from sinking, the angle of attack instantly exceeds $\alpha_{crit}$. The aircraft stalls deeply and lawn-darts straight into the ground!

Therefore, a successful hand-launch protocol requires two essential engineering rules:
1. The target launch speed must maintain a safe operational buffer above stall speed: $V_{launch} \ge 1.1 - 1.2 V_{stall}$ (our reference design specifies $V_{launch} = 11.0\text{ m/s}$).
2. The propulsion system must produce sufficient static thrust ($T/W \ge 0.35 - 0.60$) to climb at a launch angle of $\gamma \approx 10^\circ$ and accelerate forward ($a \approx 1.0\text{ m/s}^2$).

### Where you see this in the program
In **WerSeuS – UAV Calc**, these calculations are located in the **"Fixed-Wing: Stall & Hand Launch"** module:
- Entering the 2D airfoil maximum lift coefficient ($C_{l,max\_2d} = 1.25 \implies C_{L,max} = 1.125$) and Wing Loading ($W/S = 61.3\text{ N/m}^2$) automatically computes the **Sea-Level Stall Speed ($V_{stall} = 9.43\text{ m/s}$)** and **Design Altitude Stall Speed ($V_{stall} = 9.66\text{ m/s}$ at 500 m)**.
- Inputting the target launch climb gradient ($\gamma = 10^\circ$) and acceleration ($a = 1.0\text{ m/s}^2$) yields the required **Safe Launch Speed ($V_{launch} = 11.0\text{ m/s}$)** and the minimum **Launch Thrust-to-Weight Ratio ($T/W = 0.348$)**.

### Mini experiment
1. **Wing Loading and Stall Speed:** In the program, reduce the wing area ($S$) from $0.32\text{ m}^2$ to $0.20\text{ m}^2$. Notice that wing loading ($W/S$) surges, driving stall speed from $9.43\text{ m/s}$ up to $11.9\text{ m/s}$, and pushing the required hand-launch speed to an unattainable $14\text{ m/s}$ ($50\text{ km/h}$).
2. **Altitude Effect on Stall:** Raise the flight altitude from sea level to $2000\text{ meters}$ ($\rho \approx 1.007\text{ kg/m}^3$). Because air density is lower, observe that stall speed rises to $10.4\text{ m/s}$, proving that an aircraft must fly faster to avoid stalling in thin mountain air.

### Common misconceptions
1. **"A stall means the propulsion motor died":** Stall is an aerodynamic wing phenomenon, not an engine failure. An aircraft will stall at full throttle if the critical angle of attack is exceeded.
2. **"Multirotors also experience wing stalls during hand launch":** Multirotors have no fixed wings; they generate lift via direct thrust from rotating rotors and take off vertically. Stall is strictly an aerodynamic boundary-layer flow separation phenomenon specific to lifting wings.
3. **"The pilot should immediately pull up on the elevator upon hand launch":** At the moment of release, the aircraft has not yet accelerated securely past stall speed. Pulling the nose up sharply right away pushes the angle of attack past the critical angle, and the aircraft stalls and hits the ground. Instead, the aircraft should be kept on a shallow climb path (about $10^\circ$) while the motor accelerates it to a safe speed.

### Mini quiz
**1. Physically, when does an aerodynamic stall occur on a fixed-wing aircraft?**
A) When the flight battery is completely depleted
B) When the angle of attack exceeds the critical angle ($\alpha_{crit}$) and airflow separates from the upper wing surface
C) When the aircraft flies with a tailwind
D) When aircraft weight is less than wing area
*Correct Answer:* B
*Explanation:* A stall occurs when the angle of attack exceeds the critical limit, causing the boundary layer to separate from the upper surface, resulting in an abrupt loss of lift and surge in drag.

**2. For an aircraft with wing loading $W/S = 61.3\text{ N/m}^2$, maximum lift coefficient $C_{L,max} = 1.125$, and air density $\rho = 1.225\text{ kg/m}^3$, what is the sea-level stall speed ($V_{stall}$)?**
A) $4.5\text{ m/s}$
B) $9.4\text{ m/s}$
C) $18.0\text{ m/s}$
D) $25.2\text{ m/s}$
*Correct Answer:* B
*Explanation:* Evaluating the formula: $V_{stall} = \sqrt{\frac{2 \times 61.3}{1.225 \times 1.125}} = \sqrt{88.96} \approx 9.43\text{ m/s}$ (matches the Section 4 test vector).

**3. If a $2\text{ kg}$ UAV has a stall speed of $V_{stall} = 9.4\text{ m/s}$, what is a suitable target hand-launch speed ($V_{launch}$) for a safe departure?**
A) $3.0\text{ m/s}$
B) $6.0\text{ m/s}$
C) About $11.0\text{ m/s}$ (roughly 15-20% above stall speed)
D) $0\text{ m/s}$ (released with zero forward velocity)
*Correct Answer:* C
*Explanation:* A common rule keeps the launch speed 10% to 20% above stall speed ($V_{launch} \ge 1.1 - 1.2 V_{stall}$). Evaluating $9.43 \times 1.17 \approx 11.0\text{ m/s}$.
