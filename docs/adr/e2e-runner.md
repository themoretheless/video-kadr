# ADR: one browser E2E runner

- Status: accepted
- Date: 2026-09-02
- Decision: Playwright is the only product E2E runner; Cypress is not added.

## Fault scenario compared

The comparison uses one existing high-value case: the backend accepts an edit,
the job later becomes `error`, and the UI must stop polling, clear progress,
retain the edit recipe, and show the backend message.

| Concern | Playwright | Cypress |
| --- | --- | --- |
| Network fault | `page.route()` returns the submit and ordered job responses | `cy.intercept()` returns the same fixtures |
| Three engines | Chromium, Firefox, and WebKit in the existing matrix | Chromium-family and Firefox; WebKit needs a separate experimental path |
| Existing harness | Already owns an isolated Vite server and nine smoke cases | Would duplicate server lifecycle, fixtures, retries, and CI cache |
| Debug artifacts | Trace, screenshot, video, DOM snapshot | Command log, screenshot, video |
| Component tests | Vitest + Svelte client mount already cover headless/component behavior | A second component runtime would duplicate coverage |

Both runners can express the fault. Cypress provides no missing contract for
this repository, while adopting it would create two sources of truth for
timeouts, selectors, browser support, and failure artifacts.

## Consequences

All browser scenarios, including Storybook screenshots, use Playwright.
Vitest remains the component/domain runner. CI must reject Cypress config,
dependencies, or test directories unless this ADR is superseded by a measured
proposal that removes Playwright in the same change. The shared job-error
fixture belongs in the Playwright harness so product E2E and visual tests use
the same fault semantics.
