# Lesson 4: Battery Fundamentals: Wh, mAh, C-Rating, Energy Density, and Flight Endurance

### What you will learn
- The correct mathematical relationship between battery capacity (mAh), nominal voltage (V), and total stored energy (Wh).
- Why the "C-rating" is not a flight time, but rather the safe continuous maximum discharge current multiplier.
- How Depth of Discharge (DoD) and pack-level energy density (Wh/kg) directly determine UAV flight endurance.

### Explanation
When designing an electric unmanned aerial vehicle, you can think of the battery as the aircraft's fuel tank. Unlike liquid fuel tanks, however, batteries do not become lighter as they discharge; their mass remains completely constant throughout the entire flight. Therefore, calculating how long an aircraft can stay aloft requires balancing stored energy against the extra weight the battery adds: more battery means more energy, but also a heavier aircraft.

A common pitfall in UAV design is focusing solely on **milliampere-hours (mAh)**. Capacity in mAh or Ah indicates stored electric charge (for example, 1 Ah means the battery can supply 1 ampere for one hour), but it ignores voltage. The physical quantity that performs work is energy, measured in **Watt-hours (Wh)**. Energy is the product of nominal voltage and capacity in Ampere-hours ($E = V \times Ah$). For example, a single-cell $3.7\text{ V}$ battery with $10000\text{ mAh}$ ($10\text{ Ah}$) stores only $37\text{ Wh}$ of energy, whereas a $14.8\text{ V}$ (4S) battery with $5000\text{ mAh}$ stores $74\text{ Wh}$. Despite having a lower mAh rating, the 4S pack holds twice as much usable energy due to its higher voltage!

What about the **C-Rating** printed on battery packs? Contrary to popular belief, this is not an indicator of flight duration. The C-rating specifies the **maximum safe continuous discharge current** without overheating or damaging the cells. A $4.0\text{ Ah}$ battery rated at $10\text{C}$ can safely deliver up to $4.0\text{ Ah} \times 10 = 40\text{ Amperes}$ of continuous current. How long your aircraft actually stays in the air depends entirely on the electrical power your propulsion system consumes, not on the battery's maximum discharge rating.

Another crucial parameter in flight endurance calculation is the **Depth of Discharge (DoD)**. Lithium-based batteries are never discharged to zero in flight. To protect the cells from irreversible damage and avoid sudden voltage drops, a common practice is to keep roughly a 20% reserve ($DoD = 80\%$ or $0.80$). Usable energy is thus $E_{usable} = E_{pack} \times DoD$. Furthermore, cell-level gravimetric energy density (e.g., $215\text{ Wh/kg}$) degrades to approximately $170 - 190\text{ Wh/kg}$ at the pack level due to packaging, cables, solder tabs, and connectors.

Once these parameters are defined, the flight endurance is derived directly from the power balance:
$$t = \frac{E_{usable}}{P_{total,elec}} \quad [\text{hours}]$$

Here, $P_{total,elec}$ is the sum of propulsion motor power and baseline avionics consumption. For example, if a $1.0\text{ kg}$ multirotor requires $171\text{ W}$ of total electrical power to hover and carries a battery with $48.4\text{ Wh}$ of usable energy, its hover endurance is $t = 48.4 / 171 \approx 0.283\text{ hours}$, or approximately **17 minutes**.

### Where you see this in the program
In **WerSeuS – UAV Calc**, these parameters appear in the **"Power System & Battery Sizing"** module:
- Selecting the cell chemistry (such as 21700 lithium-ion cylindrical cells or LiPo packs), series/parallel cell configuration ($n_s, n_p$), and cell capacity automatically calculates the battery pack mass ($m_{batt}$) and nominal energy ($E_{pack}$).
- Applying the specified Depth of Discharge ($DoD = 0.80$) outputs the **Usable Energy ($E_{usable}$)** and calculates the predicted **Hover Endurance ($t_{hover}$)** or **Fixed-Wing Cruise Endurance ($t_{cruise}$)**.

### Mini experiment
1. **Depth of Discharge (DoD) Impact:** In the program, reduce the Depth of Discharge ($DoD$) from 80% to 60% (a conservative setting to maximize battery cycle life). Notice that usable energy and flight endurance drop by exactly 25%.
2. **Diminishing Returns of Battery Mass:** On a $1.0\text{ kg}$ multirotor, double the battery capacity (increasing battery mass from 300 g to 600 g). The total takeoff mass increases to $1.3\text{ kg}$. Because hover power scales non-linearly with weight ($P \propto W^{1.5}$), observe that the hover time does NOT double; it increases by only about 35% ($2 / 1.3^{1.5} \approx 1.35$), illustrating diminishing returns.

### Common misconceptions
1. **"A 20C battery flies the aircraft for 20 minutes":** The C-rating is not a duration; it is an allowable discharge multiplier. A 20C battery allows you to draw 20 times its capacity in current without damage. Actual flight endurance is determined strictly by how many Watts the propulsion motor draws.
2. **"Doubling battery capacity (mAh) always doubles flight endurance":** Adding more battery cells adds significant mass. The heavier aircraft requires more thrust to stay aloft, causing the motors to draw higher current. Eventually, added battery weight yields diminishing returns or prevents takeoff altogether.
3. **"Lithium batteries can be safely drained to 100% capacity in flight":** Over-discharging lithium polymer or lithium-ion cells causes irreversible internal damage and can lead to swelling and fire hazards. A common design practice is to reserve roughly 20% of the capacity ($DoD \approx 0.80$).

### Mini quiz
**1. What is the total nominal energy in Watt-hours (Wh) of a 4S battery pack with a nominal voltage of $14.8\text{ V}$ and a capacity of $4000\text{ mAh}$?**
A) $14.8\text{ Wh}$
B) $59.2\text{ Wh}$
C) $592\text{ Wh}$
D) $4000\text{ Wh}$
*Correct Answer:* B
*Explanation:* Converting units, $4000\text{ mAh} = 4.0\text{ Ah}$. The energy formula is $E = V \times Ah = 14.8\text{ V} \times 4.0\text{ Ah} = 59.2\text{ Wh}$.

**2. What is the maximum safe continuous current that can be drawn from a $5000\text{ mAh}$ ($5.0\text{ Ah}$) battery pack with a continuous discharge rating of $25\text{C}$?**
A) $5\text{ A}$
B) $25\text{ A}$
C) $125\text{ A}$
D) $500\text{ A}$
*Correct Answer:* C
*Explanation:* Maximum continuous current is $I_{max} = C \times \text{Capacity} = 25 \times 5.0\text{ Ah} = 125\text{ Amperes}$.

**3. If a UAV possesses a usable battery energy of $E_{usable} = 96.7\text{ Wh}$ and consumes a total cruise electrical power of $P_{total,elec} = 72.1\text{ W}$, what is its approximate flight endurance?**
A) 20 minutes
B) 45 minutes
C) 80 minutes (approximately 1.34 hours)
D) 150 minutes
*Correct Answer:* C
*Explanation:* Endurance is given by $t = E_{usable} / P = 96.7\text{ Wh} / 72.1\text{ W} \approx 1.34\text{ hours}$. Converting to minutes gives $1.34 \times 60 \approx 80.4\text{ minutes}$ (matches the Section 6 test vector).
