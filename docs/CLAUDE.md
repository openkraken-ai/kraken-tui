# AI Agent Instruction Manual: System Execution Guide

> **System Context:** This repository is managed via a strict 4-document planning pipeline. As an AI coding agent executing tasks within this project, your role is to implement the specifications exactly as defined. Please rely exclusively on the provided documentation to determine architecture, business logic, and dependencies.

## The 4-Document Architecture

This project separates concerns into four distinct layers. You should understand where your current task sits within these boundaries to fetch the correct context.

1. **`PRD.md` (The Conceptual Layer):** Defines the problem space. Contains the business vision, ubiquitous language (strict terminology), and explicit anti-scope (features intentionally excluded).
2. **`Architecture.md` (The Logical Layer):** Defines the system structure. Contains the logical containers, bounded contexts, fault-tolerance mechanisms, and high-level sequence flows.
3. **`TechSpec.md` (The Physical Layer):** Defines the concrete implementation. Contains the exact stack versions, database schemas (ERD), and API contracts (E.g. OpenAPI).
4. **`Tasks.md` (The Execution Layer):** Defines the logistics. Contains the atomic ticket list, the build order (dependency graph), and the Gherkin Acceptance Criteria for every feature.

---

## Documentation Routing Table

To conserve your context window and ensure accuracy, use this lookup table to find exactly what you need:

| If you need to know...                   | Target File       | Specific Section to Parse                                    |
| :--------------------------------------- | :---------------- | :----------------------------------------------------------- |
| **What task to do next?**                | `Tasks.md`        | "Build Order" & "The Ticket List"                            |
| **How to test if a task is done?**       | `Tasks.md`        | "Acceptance Criteria (Gherkin)" under the specific Ticket ID |
| **What exact tech/library to use?**      | `TechSpec.md`     | "Stack Specification" & "Implementation Guidelines"          |
| **What the database tables/types are?**  | `TechSpec.md`     | "Database Schema (Physical ERD)"                             |
| **The required JSON body/API routes?**   | `TechSpec.md`     | "API Contract (OpenAPI 3.0)"                                 |
| **How services securely communicate?**   | `Architecture.md` | "Container Diagram" & "Sequence Diagrams"                    |
| **What a specific business term means?** | `PRD.md`          | "Ubiquitous Language (Glossary)"                             |
| **If you should add an extra feature?**  | `PRD.md`          | "Boundary Analysis (Out of Scope)"                           |

---

## Execution Guidelines

1. **Interface First:** Adhere strictly to the exact types, field names, and endpoints defined in the `TechSpec.md`. If a task requires a database column or API route that does not exist in the Tech Spec, pause and request clarification from the user. Do not improvise schema or contract changes.
2. **Ubiquitous Language:** When naming variables, classes, or functions, use the exact terminology defined in the `PRD.md` Glossary. Avoid using synonyms.
3. **Definition of Done:** Code is considered complete only when it satisfies the exact `Given/When/Then` Gherkin criteria listed for your current ticket in `Tasks.md`.
4. **Scope Containment:** Focus exclusively on the current atomic task. Do not generate code or scaffold files for future tickets to prevent cascading integration issues.

## Getting Started

To begin implementation, locate your assigned Ticket ID in `Tasks.md`, review its technical requirements in `TechSpec.md`, and write the code required to satisfy the documented Acceptance Criteria.
