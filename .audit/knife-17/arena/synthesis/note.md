# Knife 17 arena synthesis note

**Base.** candidate-3.  
**Cross-judge.** Agree. Score 30/30.  
**Parent.** Same lean before judge (`'static` defer, no Table lifetime).

**Grafts applied.**

- c2: delay removing `Clone`/`PartialEq` until list commit when possible.
- c1/c4: if list slips, deliver paragraph as 17a and leave list for follow-up.

**Rejected.**

- c2 lifetime on `Cell`/`Table` (API blast for rare borrowed captures).
- c1 `FnMut` (narrower than body `FnOnce`).

**Verification.** Designs and cross-judge re-read; synthesis matches locked API above.
