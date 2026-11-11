# Lesson 6: The Design Point: Why Wing Loading (W/S) and Power Loading (P/W) Matter

### What you will learn
- The fundamental role of wing loading ($W/S$, $\text{N/m}^2$) and power loading ($P/W$, $\text{W/N}$) in preliminary aircraft sizing.
- How distinct flight mission phases (stall/landing, climb, cruise, sustained turn, and hand launch) establish aerodynamic constraints.
- How to determine the lightest, most efficient "Design Point" on a classical constraint diagram (matching chart).

### Explanation
Imagine designing a road vehicle from scratch. A city commuter car, a racing car, and a heavy cargo truck cannot share the same chassis and engine. The race car requires a massive power-to-weight ratio to accelerate instantaneously and carve through corners, while the cargo truck requires a massive load-bearing chassis and torque to haul heavy payloads. When designing a fixed-wing unmanned aerial vehicle, you cannot pick arbitrary wing dimensions or motor sizes. Aircraft sizing is governed by two foundational parameters: **Wing Loading** and **Power Loading**.

The first parameter is **Wing Loading ($W/S$)**, defined as total aircraft weight ($W = m \cdot g$) divided by reference wing planform area ($S$), expressed in $\text{N/m}^2$. Wing loading dictates how fast the aircraft must fly to stay airborne. An aircraft with high wing loading must maintain a high airspeed to generate enough lift, which elevates its stall speed and landing speed. If your mission demands hand launching in the field and landing safely in confined turf, your stall speed cannot exceed a strict threshold (e.g., $10\text{ m/s}$). This requirement imposes an upper bound on wing loading:
$$(W/S)_{max} = \frac{1}{2} \rho V_{stall,req}^2 C_{L,max}$$

The second parameter is **Power Loading ($P/W$)**, representing the required shaft power per unit weight of the aircraft ($\text{W/N}$). Every maneuver in the flight envelope demands a minimum threshold of power:
1. **Level Cruise:** Maintaining unaccelerated level flight at cruise speed; typically the least power-intensive flight phase ($P/W \approx 2.4\text{ W/N}$).
2. **Climb:** Gaining altitude against gravity (e.g., climbing at $3.5\text{ m/s}$) demands substantial mechanical power ($P/W \approx 7.6\text{ W/N}$).
3. **Sustained Turn:** In a banked turn the wing must carry more than the aircraft weight (load factor $n>1$), which raises the required lift, induced drag, and power demand.
4. **Hand Launch:** Immediately after release, the propulsion system must provide high thrust to accelerate the aircraft above stall speed along the climb path.

Aerospace engineers synthesize these requirements onto a single **Constraint Analysis Diagram (Matching Chart)** with Wing Loading ($W/S$) on the horizontal axis and Power Loading ($P/W$) on the vertical axis. The stall requirement erects a vertical boundary on the right that the design cannot cross. The climb, cruise, turn, and launch constraint curves form lower boundaries from below.

The intersection of these boundaries defining the valid solution space is the **Design Point (Sizing Point)**. For our verified $2.0\text{ kg}$ ($W \approx 19.6\text{ N}$) reference UAV, the climb requirement is the sizing driver. Applying a 15% engineering margin yields a design point of $(P/W)_{design} \approx 8.75\text{ W/N}$ at $W/S \approx 61.3\text{ N/m}^2$. This directly sizes the aircraft with a wing area of $0.32\text{ m}^2$ and specifies a motor with $172\text{ W}$ shaft power ($213\text{ W}$ electrical power draw).

### Where you see this in the program
In **WerSeuS – UAV Calc**, these calculations are visualized in the **"Fixed-Wing: Constraint Analysis & Sizing (Matching Chart)"** module:
- Entering mission specifications—Maximum Stall Speed ($V_{stall\_req} = 10\text{ m/s}$), Cruise Speed ($V_{cr} = 18\text{ m/s}$), Rate of Climb ($RC = 3.5\text{ m/s}$), and Launch Angle—automatically plots the constraint curves.
- The interactive chart highlights the boundary lines for Stall, Cruise, Climb, Turn, and Launch, pinpointing the optimal **Design Point ($W/S = 61.3\text{ N/m}^2$, $P_{shaft} = 172\text{ W}$)**.

### Mini experiment
1. **Rate of Climb Sensitivity:** In the program, increase the required rate of climb ($RC$) from $3.5\text{ m/s}$ to $5.0\text{ m/s}$. Observe the climb constraint curve shift upward, raising the required shaft power from $172\text{ W}$ to roughly $220\text{ W}$.
2. **Stall Speed Constraint:** Lower the maximum allowable stall speed ($V_{stall\_req}$) from $10\text{ m/s}$ to $8\text{ m/s}$ to enable softer field landings. Notice that the vertical stall boundary shifts to the left ($(W/S)_{max} \approx 44\text{ N/m}^2$ for $C_{L,max} \approx 1.125$), forcing the wing area ($S$) to at least about $0.45\text{ m}^2$ (the program computes the exact figure from its $C_{L,max}$ setting).

### Common misconceptions
1. **"A 500-gram UAV requires only 1 Watt of motor power (the 2 Watts per kg myth)":** This rule contradicts physics. For example, the $2\text{ kg}$ fixed-wing reference UAV requires approximately $213\text{ W}$ of electrical power during climb, and a $1\text{ kg}$ multirotor requires roughly $166\text{ W}$ to hover.
2. **"Making the wing as small as possible reduces drag, so wings should always be minimized":** Making the wing too small causes wing loading to skyrocket. Stall speed rises sharply, making hand launches difficult and increasing the risk of high-speed landing crashes.
3. **"Installing an oversized motor eliminates the need for constraint analysis":** An excessively large motor carries heavy coils, requires a heavier ESC, and demands a bulkier battery. This dead weight drives up takeoff mass, degrades wing loading, and starts a cycle that shortens endurance.

### Mini quiz
**1. Which primary constraint parameter directly dictates the maximum allowable stall speed ($V_{stall\_req}$) in fixed-wing UAV sizing?**
A) Fuselage length
B) Wing loading ($W/S$)
C) Battery C-rating
D) Propeller blade color
*Correct Answer:* B
*Explanation:* Stall speed is defined as $V_{stall} = \sqrt{\frac{2 (W/S)}{\rho C_{L,max}}}$. To ensure the stall speed does not exceed mission limits, an upper ceiling is placed directly on wing loading ($W/S$).

**2. For a UAV weighing $W = 19.6\text{ N}$ ($2.0\text{ kg}$), if the sizing power loading is $(P/W)_{design} = 8.75\text{ W/N}$, what is the required mechanical shaft power ($P_{shaft}$)?**
A) $17.2\text{ W}$
B) $87.5\text{ W}$
C) $172\text{ W}$
D) $384\text{ W}$
*Correct Answer:* C
*Explanation:* Shaft power is $P_{shaft} = (P/W)_{design} \times W = 8.75\text{ W/N} \times 19.6\text{ N} \approx 171.5\text{ W} \approx 172\text{ W}$ (matches the Section 5 test vector).

**3. What does the "Design Point" represent on an aircraft constraint diagram (matching chart)?**
A) The single point of maximum flight speed
B) The lightest and most efficient combination of wing loading and power loading satisfying all mission constraints simultaneously
C) The altitude where the battery is fully depleted
D) The point of structural wing failure
*Correct Answer:* B
*Explanation:* The design point is the optimal coordinate within the bounded solution space that minimizes motor power and structural wing size while satisfying takeoff, climb, cruise, and stall requirements.
