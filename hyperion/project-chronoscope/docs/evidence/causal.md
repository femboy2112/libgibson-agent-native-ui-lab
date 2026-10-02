## Causal attribution of divergence (one intervention per fork)

| fork | aligned steps | diverged events | intervention roots | reorder roots | first state divergence | first event divergence | outcome |
|---|---:|---:|---:|---:|---:|---:|---|
| B@50 cmd := THROTTLE | 344 | 267 | 1 | 0 | Some(50) | Some(60) | Some(StepCap)@640 |
| C@50 -cmd | 344 | 286 | 1 | 0 | Some(50) | Some(50) | Some(StepCap)@640 |
| D@62 -cmd | 344 | 191 | 1 | 0 | Some(62) | Some(62) | Some(StepCap)@640 |
| E@74 -cmd | 344 | 191 | 1 | 0 | Some(74) | Some(74) | Some(StepCap)@640 |
| F@30 +cmd THROTTLE | 344 | 309 | 1 | 1 | Some(30) | Some(30) | Some(StepCap)@640 |
| G@100 +cmd THROTTLE | 344 | 140 | 1 | 0 | Some(100) | Some(100) | Some(StepCap)@640 |
| H@200 +cmd EMERGENCY | 344 | 140 | 1 | 0 | Some(200) | Some(200) | Some(StepCap)@640 |
| I@300 +cmd THROTTLE | 344 | 43 | 1 | 2 | Some(300) | Some(300) | Some(StepCap)@640 |
| J@73 decision := 1 | 344 | 230 | 1 | 1 | Some(73) | Some(73) | Some(StepCap)@640 |
| K@271 decision := 1 | 344 | 25 | 1 | 0 | Some(271) | Some(271) | Some(StepCap)@640 |
