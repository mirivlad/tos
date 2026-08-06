<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# Process, service, and IPC model

## Process identity

A process identity includes:

- process instance ID;
- module name;
- source content ID;
- system commit ID;
- language frontend ID;
- runtime engine ID;
- granted capability set;
- parent supervisor;
- start time and restart generation.

A PID alone is insufficient for audit and debugging.

## Services

A service is a supervised process that publishes one or more versioned interfaces. A service manifest declares:

- module entry point;
- offered interfaces;
- required interfaces;
- requested capabilities;
- startup dependencies;
- restart policy;
- health probes;
- state namespace and schema;
- shutdown timeout;
- resource limits.

## Supervisors

Supervision is hierarchical.

- The boot supervisor owns essential system services.
- Driver supervisors own device-driver instances.
- Session supervisors own user applications.
- Failure propagation follows explicit policy.

Restart loops are bounded and observable. Repeated failure can mark a candidate commit unhealthy.

## IPC primitives

The nucleus provides minimal primitives:

- typed endpoint handles;
- message send and receive;
- capability transfer;
- shared-memory region transfer;
- event and interrupt notification;
- cancellation;
- process lifecycle notification.

Higher-level request/reply, streams, pub/sub, and service discovery are textual libraries and services.

## Schemas

Every IPC interface has:

- stable interface identifier;
- semantic version;
- canonical schema source;
- compatibility rules;
- maximum message sizes;
- capability-transfer declarations;
- fuzz and golden-vector tests.

Schemas are part of the system commit.

## Service discovery

Discovery returns handles, not global names with implicit authority. A process can discover only services allowed by its granted namespace capability.

## Capability transfer

Capabilities may be:

- copied when explicitly duplicable;
- moved when linear;
- attenuated to fewer rights;
- wrapped by a broker;
- revoked through an owning service where revocation semantics exist.

A numeric handle cannot be guessed to acquire authority.

## Backpressure

IPC queues are bounded. Senders receive explicit backpressure or failure. The system does not allow unbounded memory growth through message accumulation.

## State handoff

Hot service replacement can use a versioned handoff protocol. The old service may transfer:

- listening endpoints;
- in-memory session descriptions;
- durable-state transaction position;
- capability handles;
- pending work metadata.

Handoff is optional. If unsupported, the supervisor performs a clean restart.
