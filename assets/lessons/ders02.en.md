# Lesson 2: Drag, Lift-to-Drag Ratio (L/D), and Aspect Ratio

### What you will learn
- The fundamental difference between parasite drag ($C_{D0}$) and induced drag ($C_{Di}$), which is the price paid for generating lift.
- How wing aspect ratio ($AR = b^2/S$) suppresses wingtip vortices and diminishes induced drag.
- How the lift-to-drag ratio ($L/D$) dictates aerodynamic efficiency, gliding distance, and flight range.

### Explanation
Compare an albatross soaring for hours over the ocean with almost no wing flapping to a sparrow darting through a city park. Relative to its body, the albatross features extraordinarily long, slender wings, whereas the sparrow has short, stubby ones. The albatross's high-aspect-ratio wings allow it to slice through the air with minimal aerodynamic resistance, covering vast distances on gliding flight alone. Choosing this planform geometry is one of the most critical decisions when designing a fixed-wing UAV.

Every object moving through a fluid experiences a retarding aerodynamic resistance known as **Drag ($D$)**. In aircraft aerodynamics, total drag is partitioned into two distinct categories. The first is **Parasite Drag ($C_{D0}$)**, which encompasses skin friction and form drag across the fuselage, wing, and tail surfaces. It behaves like the resistance you feel when sitting upright on a bicycle against the wind: the drag force grows with the square of airspeed ($V^2$).

The second category is **Induced Drag ($C_{Di}$)**, which is the inevitable physical toll of generating aerodynamic lift. Because the air beneath a lifting wing is at higher pressure than the air above it, air spills around the wingtips from bottom to top. This lateral flow curls into swirling vortices trailing behind each wingtip, known as **wingtip vortices**. These vortices create downwash, which tilts the local lift vector backward and adds a rearward pulling component. Thus, induced drag is directly caused by the creation of lift.

This is where **Aspect Ratio (AR)** becomes paramount. Defined as the ratio of wingspan squared to wing area ($AR = b^2 / S$), aspect ratio quantifies how long and slender a wing is. Designing a wing with a high aspect ratio pushes the wingtips far apart, confining the tip vortex disturbance to a small fraction of the wing and reducing downwash. Consequently, induced drag drops significantly.

The ratio of lift to total drag defines the aerodynamic efficiency, known as the **Lift-to-Drag Ratio ($L/D$)**. We describe this mathematically using the parabolic drag polar:
$$C_D = C_{D0} + k C_L^2 \quad \text{where} \quad k = \frac{1}{\pi e AR}$$

The maximum lift-to-drag ratio is given by $(L/D)_{max} = \frac{1}{2\sqrt{C_{D0} k}}$. For example, in our verified reference design with $AR = 8.0$ and $C_{D0} = 0.0268$, the maximum glide ratio is $(L/D)_{max} \approx 13.8$. Physically, this means that if the UAV cuts its motor at an altitude of $1000\text{ meters}$, it can glide forward horizontally for roughly $13.8\text{ kilometers}$ in still air before touching the ground, at best. The same ratio means that in level flight, $13.8\text{ N}$ of weight is carried for every $1\text{ N}$ of drag.

### Where you see this in the program
In **WerSeuS – UAV Calc**, these calculations reside in the **"Fixed-Wing: Wing Geometry & Drag Polar"** module:
- By entering the **Wingspan ($b = 1.60\text{ m}$)** and **Wing Area ($S = 0.32\text{ m}^2$)**, the software calculates the aspect ratio ($AR = 8.0$).
- Based on wetted area friction estimates, the program computes the parasite drag ($C_{D0} \approx 0.0268$) and the induced drag factor ($k \approx 0.0488$), delivering the **Maximum Lift-to-Drag Ratio ($(L/D)_{max} \approx 13.8$)** and the **Best Range Speed ($V_{range} \approx 11.9\text{ m/s}$)**.

### Mini experiment
1. **Aspect Ratio Sensitivity:** Keeping the wing area fixed at $S = 0.32\text{ m}^2$, reduce the wingspan from $b = 1.60\text{ m}$ to $1.20\text{ m}$. Notice that $AR$ drops from $8.0$ to $4.5$, the induced drag factor $k$ increases, and $(L/D)_{max}$ deteriorates from approximately $13.8$ down to $10.4$.
2. **Parasite Drag Penalty:** Keep the wingspan at $1.60\text{ m}$ and imagine adding a large unstreamlined box outside the airframe (the program computes $C_{D0}$ from the fuselage and component dimensions; you only change the dimensions). As the computed $C_{D0}$ rises, observe $(L/D)_{max}$ falling and the cruise electrical power increasing. For example, if $C_{D0}$ rose from $0.0268$ to $0.0400$, $(L/D)_{max}$ would drop to about $11.3$.

### Common misconceptions
1. **"An $L/D$ of 10 means the aircraft climbs 10 units up for every 1 unit forward":** Exactly the opposite. An $L/D$ of 10 means that in an unpowered glide, the aircraft travels 10 units forward horizontally for every 1 unit of altitude lost, or generates 10 N of lift for every 1 N of aerodynamic drag.
2. **"Making the aspect ratio as large as possible is always best":** Extremely high aspect ratios (e.g., $AR > 14$) induce massive root bending moments, requiring thick structural reinforcement that adds dead weight and increases vulnerability to aeroelastic flutter. For small hobby UAVs, values between 7 and 10 offer the best compromise between structural weight and aerodynamic efficiency.
3. **"Flying faster always increases drag":** While parasite drag increases with speed, induced drag decreases as speed rises. At very low speeds, induced drag is so intense that total drag is high. Total drag reaches its minimum at the $(L/D)_{max}$ speed; flying slower or faster than this speed increases total drag.

### Mini quiz
**1. In a fixed-wing UAV, increasing the wing aspect ratio ($AR = b^2/S$) primarily reduces which type of drag?**
A) Fuselage parasite drag
B) Airfoil skin friction drag
C) Induced drag caused by wingtip vortices
D) Supersonic wave drag
*Correct Answer:* C
*Explanation:* Increasing the aspect ratio separates the wingtips further apart and suppresses the relative influence of wingtip downwash, directly decreasing induced drag ($C_{Di} = k C_L^2$).

**2. If a UAV with an $(L/D)$ of 15 loses motor power at an altitude of $500\text{ meters}$ in calm air, what is its maximum theoretical gliding distance?**
A) $1.5\text{ km}$
B) $7.5\text{ km}$
C) $15.0\text{ km}$
D) $30.0\text{ km}$
*Correct Answer:* B
*Explanation:* Gliding range is given by $R = h \times (L/D)$. Therefore, $500\text{ m} \times 15 = 7500\text{ m} = 7.5\text{ km}$.

**3. At the flight speed where the total drag force is minimized and $(L/D)_{max}$ is achieved, what is the mathematical relationship between parasite drag ($C_{D0}$) and induced drag ($C_{Di}$)?**
A) Parasite drag is zero
B) Induced drag is three times parasite drag
C) Parasite drag equals induced drag ($C_{D0} = C_{Di}$)
D) Induced drag is zero
*Correct Answer:* C
*Explanation:* On an idealized parabolic drag polar, minimum drag and peak aerodynamic efficiency occur precisely where parasite drag equals induced drag ($C_{D0} = k C_L^2$).
