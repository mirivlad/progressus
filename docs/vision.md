# Progressus — Design Bible v0.1

## 1. High concept

Progressus is a bottom-up civilization simulator. The player begins with a handful of people and a small local economy, then develops the same continuous world through agriculture, metallurgy, mechanization, industrialization, electrification, computing, automation, and later technologies.

The defining idea is not simply "RimWorld, but larger". The game should preserve the legibility and human scale of a colony simulator while allowing that colony to become a region and eventually a civilization without replacing the physical world with a separate abstract strategic layer.

## 2. Player fantasy

The intended long-term feeling is:

> I started with a few people, a fire, and primitive tools. A century later I can still find the first storehouse inside a city connected by roads, railways, power lines, factories, and distant resource settlements that all physically grew from that original place.

The world should visibly accumulate history.

## 3. Core principles

### 3.1 One world

All important entities share one world coordinate system. Characters, buildings, items, vehicles, roads, resource deposits, settlements, and infrastructure belong to the same simulation.

There is no separate expedition map or strategic overworld used to hide travel.

### 3.2 Potentially unbounded world

The world is procedurally generated in chunks as needed. Technical coordinate limits are acceptable, but normal play should never encounter an artificial edge.

Unvisited terrain must not require full storage or full simulation.

### 3.3 No abstract expeditions

If people explore a distant area, they physically travel through the world. If a mine is 40 km away, those 40 km matter.

Transport technology changes the practical scale of civilization:

- walking;
- pack animals and carts;
- roads;
- waterways;
- railways;
- motor vehicles;
- aviation;
- future automated transport.

### 3.4 Physical economy

Materials and goods have locations. Production consumes physical inputs and creates physical outputs. Logistics is not an accounting trick performed between arbitrary inventories.

The player should be able to understand why a factory is idle by following the actual chain: missing ore, insufficient transport, no fuel, no workers, broken equipment, and so on.

### 3.5 Technology is capability

Technologies primarily unlock new physical or organizational possibilities rather than flat bonuses.

Bad example:

> Electricity: +15% production.

Better example:

> Electrical generators can now be built, enabling motors, lighting, transmission networks, and later electrically powered machinery.

### 3.6 Knowledge is not enough

Advanced technology requires an enabling industrial base.

Knowing how to build an internal-combustion engine does not help if the society lacks suitable steel, precision machining, lubricants, fuel refining, measurement tools, and trained workers.

This prevents research points from allowing a primitive settlement to leap directly into advanced industry.

### 3.7 Management evolves with scale

Growing civilization must not imply linearly growing micromanagement.

Early game:

- assign jobs to individuals;
- order a few tools;
- designate trees to cut.

Middle game:

- maintain stock targets;
- configure workshops;
- create transport routes;
- assign professions and shifts.

Industrial game:

- configure production lines;
- manage throughput and regional logistics;
- define reserve levels and priorities.

Late game:

- establish rules and policies;
- automate recurring logistics and production decisions;
- manage systems rather than individual errands.

### 3.8 Society outlives individuals

The simulation spans decades and potentially centuries. Characters age, die, and are replaced by later generations.

The civilization itself is the long-lived protagonist.

### 3.9 History remains physical

Old roads, buildings, mines, settlements, and infrastructure should remain unless deliberately demolished, destroyed, buried, reclaimed, or otherwise transformed.

Important objects may retain provenance such as construction date, builder, original purpose, major renovations, and notable events.

## 4. Progression scale

The exact technology graph is a later design task, but the intended range is broad.

### Primitive society

- gathering and hunting;
- basic agriculture;
- stone tools;
- woodworking;
- pottery;
- leather and simple textiles.

### Early metallurgy

- copper;
- bronze;
- charcoal;
- mining;
- furnaces;
- forging;
- simple mechanical devices.

### Developed pre-industrial economy

- iron and improved steel;
- glass;
- water and wind power;
- better agriculture;
- increasingly precise tools and machines;
- early chemistry.

### Industrialization

- coal;
- steam power;
- machine tools;
- standardized parts;
- factories;
- railways;
- mass production.

### Electrical age

- generators;
- wiring;
- motors;
- transformers;
- electrical distribution;
- telegraphy and communications;
- electrically powered industry.

### Combustion and petrochemistry

- oil extraction;
- refining;
- engines;
- tractors;
- trucks;
- aviation;
- expanded chemical industry.

### Electronics and computing

- radio;
- electronic components;
- instrumentation;
- computers;
- control systems;
- telecommunications;
- industrial automation.

### Atomic and advanced industry

- advanced materials;
- nuclear power;
- high-precision manufacturing;
- robotics;
- large-scale automation.

### Late technologies

Possible future directions include fusion, advanced robotics, synthetic materials, orbital infrastructure, and space industry. These are not commitments for early development.

## 5. Technology graph

The technology system should be a dependency network rather than a single linear tree.

A capability may depend on several domains at once. For example, semiconductor manufacturing may require progress in:

- chemistry;
- purified materials;
- optics;
- vacuum systems;
- precision mechanics;
- electrical engineering;
- measurement and control;
- clean production environments.

The technology graph therefore interacts with the production graph.

## 6. Knowledge and education

Knowledge should initially exist partly in people.

Characters may possess skills and practical knowledge such as agriculture, medicine, carpentry, metallurgy, mechanics, chemistry, and electrical work.

As society develops, knowledge becomes easier to preserve and distribute through:

- oral teaching;
- writing;
- books;
- schools;
- libraries;
- technical documentation;
- universities;
- standards;
- digital archives.

The death of a unique specialist may be disastrous early on but relatively unimportant in a mature society with robust institutions.

This system is important, but deep educational simulation is not required for Prototype 01.

## 7. Production

Production chains should be materially meaningful.

A complex product is made from parts and processes, but gameplay must avoid forcing the player to manually schedule every screw and washer.

Example conceptual chain:

vehicle

→ engine, transmission, body, wheels, electrical system, cooling, fuel system

engine

→ cast/machined block, pistons, crankshaft, bearings, valves, fasteners

Those dependencies matter to the economy, while automation and stock rules keep the interface manageable.

## 8. Resources

Resources occupy the world and help determine settlement geography.

Examples:

- forests;
- water;
- arable soil;
- stone;
- clay;
- limestone;
- copper ore;
- iron ore;
- coal;
- oil;
- salt;
- rare metals.

A distant resource deposit should create a real logistical decision and potentially lead to roads, camps, new settlements, industries, and eventually new towns.

## 9. Logistics

Logistics is a first-class system.

Every physical cargo has at least:

- type;
- quantity or stack state;
- current location;
- source context;
- destination or demand context when assigned.

Transport methods evolve over time, but the same underlying rule remains: material must travel through the world.

Distance, capacity, infrastructure, congestion, and handling costs should eventually matter.

## 10. Settlements and regions

A settlement should emerge from the simulation rather than being primarily an arbitrary map token.

A remote mine may begin as a work camp. Permanent housing and services may turn it into a village. New industry may transform it into a town. Several neighboring settlements may become a larger urban region.

The player ultimately builds a region, not merely one base.

## 11. Population

Target simulation scale is much larger than a traditional colony simulator:

- 5 people;
- 20;
- 100;
- 1,000;
- 10,000;
- potentially much more.

The same detail level cannot be used at every scale.

## 12. Simulation LOD

Simulation detail must depend on relevance.

### LOD 0 — active detailed simulation

Around the player focus and other critical regions:

- individual characters;
- explicit tasks;
- physical items;
- pathfinding;
- detailed interactions.

### LOD 1 — reduced local simulation

Nearby but inactive areas retain individual entities while some low-value processes are batched or simplified.

### LOD 2 — settlement aggregation

Remote settlements may aggregate household consumption, routine production, and other stable processes while preserving important individuals and state transitions.

### LOD 3 — regional aggregation

Very distant stable regions may update via larger time steps using flows and inventories rather than simulating every movement.

Any aggregation must preserve quantities and meaningful state well enough that returning to detailed simulation does not create obvious contradictions or free resources.

## 13. Characters

A minimal character model includes:

- stable identity;
- name;
- age;
- health;
- current position;
- current job/task;
- skills;
- profession or work role;
- residence when applicable.

Later systems may include family, relationships, personality, ideology, culture, and preferences. These are secondary to the economic and civilizational core.

## 14. Needs

Initial needs should remain understandable:

- food;
- water where the world model requires it;
- sleep;
- temperature/shelter;
- health;
- safety.

Later additions can include comfort, recreation, housing quality, and social needs.

Progressus should not become primarily a psychological-break simulator. Human needs matter because society matters, but economic and technological development remains central.

## 15. Construction

Buildings and infrastructure are physically constructed from materials and labor.

Technology changes construction itself: better tools, machinery, cranes, standardized parts, concrete, prefabrication, and later automation can increase achievable scale.

## 16. Energy

Energy evolves through the technological progression:

- human labor;
- animal power;
- water and wind;
- steam;
- electricity;
- liquid fuels;
- nuclear energy;
- later sources.

Electrical systems should eventually include generation, distribution, conversion, capacity, and consumers. Early implementations may deliberately simplify electrical engineering while preserving the concept of a physical network.

## 17. World generation

The world is deterministic from a seed.

Likely world properties include:

- terrain height;
- water and drainage;
- climate;
- temperature and rainfall;
- biomes;
- soil;
- vegetation;
- fauna;
- mineral deposits.

The same seed and world-generation version must reproduce the same untouched world.

Modified chunks are persisted as deviations from that deterministic base wherever practical.

## 18. Mutable world

Civilization changes the world.

Potential changes include:

- deforestation;
- farms;
- roads;
- quarries and mines;
- pollution;
- changed waterways;
- buildings and ruins;
- abandoned infrastructure;
- urban growth.

The world is not a disposable level. It is the historical record of the simulation.

## 19. Time and history

The game must support long timelines without requiring every distant routine process to be stepped at the finest granularity.

Important entities may preserve metadata such as:

- created/built date;
- creator/builder;
- original purpose;
- ownership history;
- major renovations;
- important incidents.

The purpose is not exhaustive event logging. The purpose is for the world to tell its own history.

## 20. Trade and other societies

External societies are not required for the first prototypes.

If added later, trade and interaction should respect the same world model. Goods should arrive through real routes rather than magical map-edge events where practical.

Other societies may eventually include tribes, towns, states, companies, or competing civilizations.

War may exist but is not the project's central theme.

## 21. Victory and failure

Progressus does not require a single mandatory victory condition.

Milestones such as first steel, first railway, first electrical grid, first computer, first reactor, or first orbital launch can mark achievements without ending the simulation.

Complete population loss is a natural defeat. Severe crises may instead cause contraction, abandonment, fragmentation, or technological regression and still allow recovery.

## 22. Interface philosophy

The UI must support transitions in scale:

individual → workplace → settlement → city → region → civilization.

Different scales should expose different controls rather than presenting tens of thousands of individual RimWorld-style task toggles forever.

## 23. Core gameplay loop

A representative emergent loop is:

need

→ resource acquisition

→ production

→ infrastructure

→ growth

→ new constraints

→ new needs

Example:

More food is needed.

→ Farms expand.

→ More tools are needed.

→ Metalworking expands.

→ More ore is needed.

→ A distant mine is opened.

→ Transport becomes inadequate.

→ A road or railway is built.

→ A worker settlement emerges near the mine.

→ That settlement creates new demand.

The game should create these chains through interacting systems rather than authored event scripts whenever possible.

## 24. What Progressus is not

### Not a RimWorld clone

RimWorld is a useful reference for readable colony-scale play, not the desired endpoint.

### Not Factorio with citizens

Automation and throughput matter, but people, institutions, geography, and history remain essential.

### Not Civilization on a tile map

Cities are not abstract production tokens. Their buildings, roads, industries, and people physically exist in the simulated world.

### Not depth for depth's sake

The project can learn from Dwarf Fortress, but a simulated detail is valuable only if it creates useful consequences, interesting choices, believable history, or stronger systemic behavior.

## 25. Development philosophy

Do not attempt the dream game in one implementation pass.

Every milestone should prove one layer of the concept and remain a coherent smaller game:

1. world + characters + jobs + physical resources;
2. sustainable settlement economy;
3. multiple settlements + scalable logistics + simulation LOD;
4. industrial region;
5. broader civilization systems.

Build architecture that permits the future, but do not write speculative late-game systems until an accepted milestone needs them.

## 26. Project test

For any major feature, ask:

> Does this help the player experience the growth of a small settlement into a civilization inside one continuous physical world?

If the answer is no, it is probably not a priority.
