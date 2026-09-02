# Accessibility interaction matrix

Automated axe checks are release gates for the normal shell, backend error
state, shortcut dialog and timeline command menu. Serious and critical WCAG
2.0/2.1 A/AA findings fail Playwright.

| Surface | Keyboard | Screen reader contract | Pointer parity |
| --- | --- | --- | --- |
| Timeline toolbar | roving Left/Right, Home/End | toolbar label, command label, shortcut and disabled reason | click/tap |
| Clip move | Left/Right by frame | selected clip and exact range in semantic list | mouse/touch/pen drag |
| Clip trim | handle Left/Right by frame | named start/end handles | mouse/touch/pen handle drag |
| Clip slip | Shift+Left/Right by frame | fixed timeline range, source changes only | Alt+mouse/pen drag |
| Timeline scale | native range keys | labeled zoom value | drag/tap |
| Semantic alternative | Tab/Enter | ordered clips grouped by labeled track | click/tap selection |
| Command menu | Enter, Escape, Tab | menu/menuitem state from shared registry | click/tap |
| Shortcut dialog | Tab, Escape, capture cancel | modal name/description, live status/error | click/outside close |

Manual release review remains required for VoiceOver and NVDA announcement
quality, focus order at 200% zoom, high-contrast mode, touch target size and
caption/audio-description accuracy. Passing axe does not replace that review.
