# Lesson 8: Wing Spars and Multirotor Arms: Why Do They Break or Vibrate?

### What you will learn
- How aerodynamic lift generates a massive root bending moment ($M_b$) in fixed-wing UAVs and how internal wing spars carry this load.
- How to compute section modulus ($Z$), peak bending stress ($\sigma$), and Margin of Safety ($MS = \sigma_{allow}/\sigma - 1$).
- How multirotor arms respond to motor thrust and reaction torque, why arm stiffness prevents resonance, and its impact on flight controller gyroscopes.

### Explanation
Imagine holding a heavy grocery bag with your arm hanging straight down against your torso; you can sustain the weight with little effort. Now imagine extending that same bag horizontally straight out to your side: within seconds, your shoulder experiences an unbearable strain. The weight of the bag has not changed, but the length of your outstretched arm acts as a powerful lever arm, generating a massive bending moment at your shoulder joint. Aircraft wings and multirotor arms experience this exact same bending physics continuously while flying.

When a fixed-wing aircraft pulls through a high-G turn or encounters severe atmospheric turbulence, aerodynamic lift distributed across the wingspan accumulates toward the fuselage. While aircraft weight pulls downward at the center of gravity, aerodynamic lift pulls upward across the wings. This creates a severe **Root Bending Moment ($M_b$)** that peaks where the wings join the fuselage. A lightweight aerodynamic skin cannot carry this bending load alone. To survive, the wing must be reinforced internally with a rigid structural backbone: the **Wing Spar**.

In aerospace structural engineering, airframes are designed not just for cruise flight, but for the **ultimate load factor ($n_{ult} = n_{limit} \times FS = 3.8 \times 1.5 = 5.7$)**, combining maneuvering limits with a 1.5 factor of safety. For a $2.0\text{ kg}$ UAV ($W \approx 19.6\text{ N}$) with a $1.6\text{ m}$ wingspan and elliptical lift distribution, the ultimate root bending moment for a half-wing reaches roughly $19.0\text{ N}\cdot\text{m}$:
$$M_{b,ult} = \frac{n_{ult} W b}{3 \pi}$$

To resist this moment without excessive weight, engineers employ hollow cylindrical carbon fiber tubes (e.g., outer diameter $D_o = 10\text{ mm}$, inner diameter $D_i = 8\text{ mm}$). The structural resistance of the hollow tube is quantified by its **Section Modulus ($Z$)**:
$$Z = \frac{\pi}{32} \frac{D_o^4 - D_i^4}{D_o} \approx 5.80 \times 10^{-8}\text{ m}^3$$

The peak bending stress induced in the carbon tube is $\sigma_{max} = M_{b,ult} / Z \approx 327\text{ MPa}$. In this example the allowable strength of the carbon tube is taken as $\sigma_{allow} = 500\text{ MPa}$ (real values depend on the material data). The structural **Margin of Safety (MS)** is:
$$MS = \frac{\sigma_{allow}}{\sigma_{max}} - 1 = \frac{500}{327} - 1 = +0.53$$
Because $MS \ge 0$, the structural spar will safely carry the ultimate aerodynamic loads without yielding or breaking.

In multirotors, the motor arms function as cantilever beams subjected to upward motor thrust ($T_{max}$) and reactive motor torque ($Q$). While carbon fiber arms rarely snap under static loads, the primary structural hazard is **Stiffness and Vibration**. Brushless motors spinning at tens of thousands of RPM generate high-frequency vibration. If the arm lacks sufficient bending and torsional stiffness, its natural structural frequency will coincide with motor rotational harmonics. This triggers resonance, causing the arms to vibrate strongly. This mechanical noise corrupts the readings of the flight controller's MEMS gyroscopes, so the control loop sends wrong corrections, causing motor jitter, overheating, or loss of control.

### Where you see this in the program
In **WerSeuS – UAV Calc**, these structural calculations appear across two modules:
- **Fixed-Wing: Structural Sizing & Spar Analysis:** Entering aircraft mass, wingspan, limit load factor ($n_{limit} = 3.8$), and tube dimensions ($D_o = 10\text{ mm}, D_i = 8\text{ mm}$) computes the **Root Bending Moment ($19.0\text{ N}\cdot\text{m}$)**, **Peak Bending Stress ($327\text{ MPa}$)**, and the **Margin of Safety ($MS = +0.53$)**.
- **Multirotor: Arm Geometry & Vibration Analysis:** Entering the wheelbase ($D_{frame}$), arm length ($L_{arm} = \frac{D_{frame}}{2} - R_{plate}$), and motor RPM flags cantilever deflection and potential resonance hazards.

### Mini experiment
1. **Spar Diameter and Stress:** In the fixed-wing module, decrease the carbon spar outer diameter from 10 mm to 8 mm (with a 6 mm inner diameter). Notice that the section modulus ($Z$) drops from about $5.8 \times 10^{-8}$ to $3.4 \times 10^{-8}\text{ m}^3$, bending stress rises to about $550\text{ MPa}$, and the Margin of Safety turns negative ($MS \approx -0.09$), meaning this spar would be inadequate at ultimate load.
2. **Multirotor Arm Stiffness:** In the multirotor module, increase the frame diameter from $250\text{ mm}$ to $450\text{ mm}$ while keeping arm wall thickness constant. Observe that cantilever deflection scales with the cube of length ($L^3$), prompting the software to display an arm stiffness / resonance risk warning.

### Common misconceptions
1. **"A wing spar is the same thing as wing camber or angle of attack":** A spar is not an aerodynamic shape. It is the primary internal load-bearing structural beam that carries bending forces; its role is structural strength, not lift generation.
2. **"As long as a component does not break under static load, its flexibility does not matter":** In multirotors, excessive arm flexibility lowers the natural frequency of the structure into the operating range of motor RPM. Resonant vibration corrupts (and can saturate) the flight controller's gyro and accelerometer readings, destabilizing the vehicle even if the material never breaks.
3. **"A negative Margin of Safety ($MS$) indicates a safe structure":** In aerospace engineering, Margin of Safety is defined as $MS = (\sigma_{allow}/\sigma_{max}) - 1$. A value of $MS \ge 0$ represents a safe design, whereas $MS < 0$ denotes impending structural failure under design limit loads.

### Mini quiz
**1. What is the dominant structural load developed at the wing root of a fixed-wing UAV during high-G flight maneuvers?**
A) Pure axial tension
B) Root bending moment ($M_b$)
C) Nose stagnation impact pressure
D) Thermal solar radiation stress
*Correct Answer:* B
*Explanation:* Distributed aerodynamic lift acting across the span creates a cantilever lever-arm effect that generates an intense root bending moment ($M_b$) where the wing joins the fuselage.

**2. If an ultimate bending load produces a stress of $\sigma_{max} = 327\text{ MPa}$ in a carbon spar and the allowable material strength is $\sigma_{allow} = 500\text{ MPa}$, what is the Margin of Safety ($MS$)?**
A) $-0.35$
B) $+0.53$
C) $+1.53$
D) $0.00$
*Correct Answer:* B
*Explanation:* The Margin of Safety formula is $MS = (\sigma_{allow} / \sigma_{max}) - 1$. Evaluating $(500 / 327) - 1 = 1.529 - 1 \approx +0.53$ (matches the Section 9 test vector).

**3. What is the most critical hazard caused by multirotor arms with inadequate structural stiffness?**
A) The battery suddenly cools down
B) Arm flexibility causes resonance with motor frequencies, blinding the flight controller gyroscopes
C) The propellers begin spinning in reverse
D) The overall mass of the drone spontaneously increases
*Correct Answer:* B
*Explanation:* Flexible arms resonate with motor rotational harmonics, transmitting violent mechanical vibration to the airframe that saturates the flight controller's MEMS gyro filters and leads to loss of control.
