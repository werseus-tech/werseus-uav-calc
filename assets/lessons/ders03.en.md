# Lesson 3: Center of Gravity, Neutral Point, and Static Margin: Why Does an Airplane Stay in Balance?

### What you will learn
- The physical meaning of the Center of Gravity ($x_{cg}$), Aerodynamic Center ($x_{ac}$), and Neutral Point ($x_{np}$) in fixed-wing aircraft.
- Why the Static Margin ($SM = (x_{np} - x_{cg}) / MAC$) must be between 5% and 15% to guarantee positive longitudinal static stability.
- How Center of Gravity eccentricity ($\Delta_{cg} \le 3\%$) affects motor load balancing and flight safety in multirotors.

### Explanation
Picture a standard throwing dart or a rooftop weather vane. When you throw a dart, why does the heavy metal tip always lead the way while the lightweight stabilizing fins trail behind? Even if you intentionally throw it backward, it immediately flips nose-first in mid-air. The stabilizing aerodynamic fins behind the center of gravity create a restoring lever arm that constantly realigns the dart with the airflow. This very principle keeps a fixed-wing aircraft flying smoothly without tumbling uncontrollably.

The balance point where the entire gravitational mass of the aircraft concentrates is the **Center of Gravity ($x_{cg}$)**. The isolated wing produces its lift around its aerodynamic center ($x_{ac\_w}$), typically near the 25% mean aerodynamic chord (MAC) point. However, when we add a fuselage and a horizontal tail stabilizer, the aerodynamic balance point of the complete aircraft shifts aft. The point along the chord where the aircraft's net pitching moment remains independent of angle of attack is called the **Neutral Point ($x_{np}$)**.

For an aircraft to exhibit inherent pitch stability (positive longitudinal static stability), the **Center of Gravity must always be located AHEAD of the Neutral Point ($x_{cg} < x_{np}$)**. If an unexpected gust pitches the nose upward, the additional aerodynamic lift generated at the neutral point (behind the CG) acts like a lever arm, pitching the nose back down automatically to restore trim equilibrium ($C_{m\alpha} < 0$). Conversely, if the CG is aft of the neutral point, an upward pitch disturbance creates an unstable moment that forces the nose even higher, leading to a divergence that the pilot can hardly control.

Aerospace engineers quantify this safety buffer using the non-dimensional **Static Margin ($SM$)**:
$$SM = \frac{x_{np} - x_{cg}}{MAC}$$

Here, $MAC$ denotes the Mean Aerodynamic Chord of the wing. For small UAVs and hobby aircraft, the target static margin is typically chosen between **8% and 12% MAC** (with 10% as standard). If the static margin exceeds 15%, the plane becomes excessively nose-heavy, requiring excessive elevator deflection that increases trim drag; if it drops below 5%, the aircraft becomes twitchy and dangerously difficult to fly.

In multirotors, hover dynamics are governed differently because there is no wing or aerodynamic neutral point in hover. However, balance remains critical: the center of gravity should sit at the geometric center of the motor layout $(0,0)$. A practical rule is to keep the CG offset within 3% of the diagonal wheelbase ($d_{cg} \le 0.03 \cdot D_{frame}$). With a larger offset, the motors on the loaded side must spin continuously faster, which heats them up and reduces the power headroom the controller has for maneuvering.

### Where you see this in the program
In **WerSeuS – UAV Calc**, these calculations are performed in the **"Centre of Gravity & Stability"** module:
- By inputting the wing and tail geometry, the software calculates the horizontal tail volume ratio ($V_H = S_h l_h / (S \cdot MAC)$) and locates the **Neutral Point ($x_{np} = 51.2\%\text{ MAC}$)**.
- Setting the desired **Static Margin ($SM = 10\%$)** establishes the required target center of gravity at **$x_{cg\_target} = 41.2\%\text{ MAC}$**.
- The program's "Battery Solver" computes where the flight battery must be placed along the fuselage ($x_{batt}$, in millimeters) to reach the target CG.

### Mini experiment
1. **Instability Threshold:** In the program, shift the battery position ($x_{batt}$) rearward until the center of gravity moves behind the neutral point ($SM = -5\%$). Observe that the software triggers a critical warning: **"Static margin ... outside green band (...). CG too far aft: move battery forward."** (in simple mode: **"unstable: it will not fly like this"**).
2. **Tail Volume Effect:** Reduce the horizontal tail area ($S_h$) from $0.055\text{ m}^2$ to $0.030\text{ m}^2$. Notice that the neutral point ($x_{np}$) shifts forward closer to the wing, forcing you to push the heavy battery further toward the nose to preserve balance.

### Common misconceptions
1. **"The further forward the CG is, the safer the airplane flies":** Moving the CG too far forward ($SM > 20\%$) makes the aircraft severely nose-heavy. The elevator loses authority to raise the nose, resulting in high landing speeds or nose-first crashes during hand launches.
2. **"The neutral point is simply the geometric midpoint of the fuselage":** The neutral point is not a physical midpoint. It is an aerodynamic property calculated from wing and tail lift curves, surface areas, tail moment arms, and downwash interactions.
3. **"Because multirotors have flight controllers, battery placement does not matter":** While the autopilot can compensate for a misplaced CG by spinning the loaded motors faster, this causes asymmetric motor saturation, high heat, and potential loss of control during sudden wind gusts or full-throttle climbs.

### Mini quiz
**1. What is the fundamental requirement for a fixed-wing UAV to possess positive longitudinal static stability?**
A) The Center of Gravity ($x_{cg}$) must be located ahead of the Neutral Point ($x_{np}$) ($x_{cg} < x_{np}$)
B) The Center of Gravity must be at the trailing edge of the wing
C) Wing area must be smaller than fuselage wetted area
D) The motor must be mounted in a pusher configuration
*Correct Answer:* A
*Explanation:* Positive longitudinal static stability requires the CG to be ahead of the neutral point ($x_{cg} < x_{np}$) so that any upward pitch disturbance generates a nose-down aerodynamic restoring moment ($C_{m\alpha} < 0$).

**2. On an aircraft with a Mean Aerodynamic Chord $MAC = 200\text{ mm}$, the neutral point is at $x_{np} = 100\text{ mm}$ and the center of gravity is at $x_{cg} = 80\text{ mm}$. What is the Static Margin ($SM$)?**
A) 5%
B) 10%
C) 20%
D) -10%
*Correct Answer:* B
*Explanation:* Static Margin is defined as $SM = (x_{np} - x_{cg}) / MAC$. Evaluating $(100 - 80) / 200 = 20 / 200 = 0.10$, or exactly **10%**.

**3. In a multirotor UAV, what is the maximum recommended Center of Gravity offset relative to the diagonal frame diameter ($D_{frame}$)?**
A) 25% of the frame size
B) 10% of the frame size
C) No more than 3% of the diagonal motor-to-motor frame size ($d_{cg} \le 0.03 \cdot D_{frame}$)
D) Center of gravity offset has zero influence on multirotors
*Correct Answer:* C
*Explanation:* To prevent motor load imbalance and maintain sufficient flight controller dynamic headroom, CG eccentricity must remain within 3% of the diagonal wheelbase ($D_{frame}$).
