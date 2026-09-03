# Production — design philosophy

Status: **Normative design guidance**

Progressus has physical production and logistics, but it is **not a factory-chain optimization game**.

The purpose of production is to make technological, territorial, and social development believable. Production should create understandable dependencies and reasons for settlements, professions, infrastructure, trade, and technological progress. It should not become a puzzle whose main challenge is perfectly matching long chains of intermediate products.

## 1. Default rule: model the meaningful step

A recipe or production step should exist when it creates at least one useful gameplay consequence, such as:

- requiring a new material;
- requiring a new craft, profession, machine, or facility;
- creating a meaningful logistics dependency;
- enabling specialization between settlements or industries;
- creating maintenance or supply concerns that matter at the current scale;
- marking a significant technological capability;
- producing an object players can understand and care about.

Do **not** add an intermediate component only because the real-world product contains it.

Historical or engineering accuracy is a source of ideas, not a requirement to expose every manufacturing step to the player.

## 2. Complexity budget

Production depth has a complexity budget.

When deciding whether to split a product into more components, ask:

> Does this create an interesting decision, dependency, or story, or does it merely make the recipe longer?

If the answer is only "it is more realistic", prefer the simpler model.

Examples:

### Good simplification

A blacksmith may turn iron/steel, fuel, labor, and access to a forge into ordinary tools without separately producing every handle, rivet, pin, and fastener.

### Good explicit intermediate

Steel may be distinct from iron because obtaining steel represents a major technological and industrial capability and changes what society can build.

### Good explicit component

An engine may be a meaningful produced component of a vehicle because engines require specialized industry, can be stocked/repaired/traded, and can serve several vehicle or machine types.

### Usually excessive

Requiring the player economy to separately manufacture piston rings, individual valve springs, washers, bolts, wire terminals, and every other minor part unless a later design has a strong gameplay reason for doing so.

## 3. No conveyor-belt assumption

Progressus must not assume that production is organized like Factorio-style continuous belt lines.

Valid production organization may include:

- one person crafting at a bench;
- a village workshop producing on demand;
- craftsmen maintaining local stock targets;
- a manufactory receiving periodic deliveries;
- a factory operating in batches or continuously;
- distributed suppliers serving several towns;
- later automated industry.

Transport and storage matter, but they need not be expressed through visually continuous conveyor networks.

## 4. Throughput is a consequence, not the main fantasy

Capacity and bottlenecks can matter. A railway may be unable to carry enough coal. A steelworks may not receive enough ore. A city may outgrow its food supply.

These situations are desirable because they connect geography, infrastructure, population, and technology.

However, the game should not require routine mathematical balancing of every machine ratio to remain enjoyable.

Approximate capacity, buffers, stock targets, scheduling, and automation should allow a reasonably designed economy to function without constant tuning.

Optimization remains available to players who enjoy it, but it is not mandatory play.

## 5. Prefer legible causes

When production stops, the cause should be understandable:

- no suitable raw material;
- no fuel or power;
- no worker with the necessary skill;
- required facility unavailable;
- transport cannot deliver supplies;
- storage is full;
- equipment is damaged;
- required technology/knowledge is unavailable.

Avoid failures caused by invisible recipe bookkeeping or unnecessarily granular components.

## 6. Technology and production

Technology should usually unlock **capabilities** rather than simply add more recipe nodes.

For example, industrial steelmaking can introduce:

- stronger structural materials;
- rails;
- better machine tools;
- pressure vessels;
- improved vehicles and machinery.

The important gameplay event is society gaining reliable steelmaking capacity, not the player completing a long chain for its own sake.

## 7. Scale changes abstraction

The appropriate production detail changes as civilization grows.

Early game may show a named craftsperson producing specific objects.

Later, the player may manage:

- workshop orders;
- minimum stock levels;
- industrial capacity;
- regional supply priorities;
- contracts/routes;
- automated replenishment.

Scaling management should reduce repetitive micromanagement rather than exposing ever more recipe bookkeeping.

## 8. Production orders and shared inputs

A workstation may expose persistent production orders rather than requiring the player to issue one craft command per item. The bootstrap supports finite remaining counts and an explicit infinite order that continues while physical inputs and workers are available. Later stock-target or conditional policies may extend this model.

An infinite order must not mean that one workstation owns all future resources. Physical input reservations belong to one concrete Craft job at a time. When two workstations can use the same stack, only one may reserve it; the other waits. Scheduling should remain deterministic and avoid starvation in ordinary shared-input cases. Ingredients are never shuttled back and forth merely because multiple workstations can use them.

The UI should make the reason for waiting legible: missing inputs, unavailable worker, occupied workstation, or another reservation should be inspectable rather than hidden behind generic inactivity.

## 9. Design guardrail for agents

When implementing production content:

1. Start with the smallest materially believable recipe.
2. Add intermediate products only when their independent existence has a gameplay purpose.
3. Do not introduce conveyor-belt or exact-ratio assumptions into generic production APIs.
4. Do not make a deeper chain merely because another factory game models it that way.
5. If a proposed production feature substantially increases recipe depth across the game, treat it as a design decision and document the reason before implementation.

## 10. Reference point

Progressus may borrow from factory games the useful ideas of physical inputs, stock, capacity, bottlenecks, automation, and transport.

It deliberately does **not** adopt factory-chain optimization as its central gameplay loop.

The central loop remains the growth of a living settlement into a civilization in one continuous world.
