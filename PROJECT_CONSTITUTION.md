DirectPaymentTimesheets

Project Constitution

Version: 1.0
Date: 31 July 2026

;

1. Project Purpose

DirectPaymentTimesheets is a project to create a reliable, maintainable system for managing Direct Payment timesheets.

The purpose is to replace manual, error-prone processes with a structured digital workflow that improves accuracy, traceability, and ease of use.

The system should prioritise:

* simplicity
* reliability
* accessibility
* maintainability
* clear documentation

;

2. Project Principles

2.1 Keep It Simple

The system should avoid unnecessary complexity.

Every feature should justify:

* why it exists
* who benefits from it
* what problem it solves

Complexity should only be introduced when it provides a clear advantage.

;

2.2 Document Before Building

Important decisions must be recorded before implementation.

Documentation should explain:

* what is being built
* why it is being built
* how it works
* how it can be maintained

;

2.3 Small Controlled Changes

Changes should be introduced in small steps.

Each meaningful change should:

* have a clear purpose
* be tested where possible
* be committed separately in Git

Large uncontrolled changes should be avoided.

;

2.4 Reliability Over Speed

The system should favour dependable operation over rapid development.

A slower but reliable solution is preferable to a quick solution that creates future problems.

;

3. Development Rules

Source Control

GitHub will be the official source repository.

All significant development work should be committed with meaningful commit messages.

Examples:

Good:

Add timesheet validation rules

Poor:

stuff

;

Branching

The main branch represents the stable project.

Experimental work should use separate branches where appropriate.

;

4. File Organisation

The project should maintain a clear separation between:

docs/

Documentation and decisions.

src/

Application source code.

tests/

Testing material.

;

5. Security Principles

The project must never store:

* passwords
* private keys
* personal confidential data
* access tokens

inside the repository.

Sensitive information must be stored separately.

;

6. Accessibility Principles

The system should consider users who may have:

* limited mobility
* reduced technical ability
* accessibility requirements

Interfaces should minimise unnecessary effort.

;

7. Change Management

Before adding major features:

1. Define the problem.
2. Describe the proposed solution.
3. Consider alternatives.
4. Record the decision.

;

8. Project Success Criteria

The project will be considered successful when it provides:

* accurate timesheet management
* reduced manual effort
* clear records
* reliable operation
* maintainable code
* understandable documentation

;

9. Ownership

This constitution defines the guiding principles for the DirectPaymentTimesheets project.

Future development should follow these principles unless a documented decision is made to change them.

