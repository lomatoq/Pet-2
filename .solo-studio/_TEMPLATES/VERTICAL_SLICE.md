# Vertical Slice Plan — {{PROJECT_NAME}}

Updated: {{DATE}}

## Slice verdict question

> Can the game promise reach near-final player experience at sustainable production cost?

## Golden path

- Start state:
- Duration:
- Player objective:
- Required actions:
- Representative challenge:
- Failure:
- Recovery:
- End state:
- Fast replay/reset:

## In scope

- Gameplay:
- Camera:
- Animation:
- VFX:
- Audio:
- UI:
- Art:
- Persistence:
- Tools/debug:
- Performance:
- Playtest:

## Explicitly out of scope

-
-
-

## State and data ownership

```text
input → intent → simulation → events/snapshot → presentation → UI/audio/VFX
```

- Input owner:
- Domain owner:
- Physics owner:
- Presentation owner:
- Save relevance:
- Authoring data:
- Experimental seam:
- Fallback:

## Representative quality bar

- Target reference:
- Visual thesis:
- Target resolution:
- Target hardware:
- FPS/frame time:
- Memory:
- Loading:
- Accessibility/input variants:

## Asset and content throughput

Representative unit:
- Creation time:
- Integration time:
- Review/fix time:
- Reuse:
- Tooling required:
- Forecasted total:

## Experiments

1. Question:
   - Baseline:
   - Metric:
   - Success:
   - Kill:
   - Fallback:

## Instrumentation

- Debug views:
- Logging:
- Profiler captures:
- Fixed seed/scenario:
- Screenshot/video capture:
- Automated smoke:
- Visual regression:

## Playtest

- Participant:
- Hypothesis:
- Scenario:
- Observed metrics:
- Questions after behavior:
- Decision rule:

## Acceptance

- [ ] Promise is legible without explanation.
- [ ] Core toy remains satisfying without meta rewards.
- [ ] Golden path is launchable and replayable.
- [ ] Input/camera/feedback feel near final.
- [ ] Representative art passes target-size review.
- [ ] Target hardware budget passes.
- [ ] Content throughput is measured.
- [ ] Largest risk has evidence.
- [ ] Fallback is proven or credible.
- [ ] Cut line and next gate are decided.

## End decision

Continue | Narrow | Pivot | Kill

Evidence:
