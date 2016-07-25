# Overview

## Lifecycle and book keeping.
Gecko doesn't do any specific book keeping, but at the foxbox level the behavior depends on the worker's kind:

- Web workers: a web worker is kept alive by the refcount of clients that started it and didn't terminate() it. If the worker itself calls close() it shuts down all clients. There is no need to restart web workers at startup on the foxbox or jsrunner side.

- Service workers: these follow the register/unregister cycle. Registering twice the same SW is idempotent, thus foxbox can register them at startup so that gecko doesn't have to do any special book keeping.

## Public urls
Service workers scopes are rooted at $domain/user/$user_id/ which can be considered as a "virtual base" for each user.

# TODO

## Foxbox:
- Turn into an adapter?
- More tests.
- cleanup of workers.rs

## Gecko:
- Create the worker with a per-usercontext principal.
- Worker cache policy?
- Tests.

## Web library
- Authentication.

## All:
- webrtc mode.

## Next
- Service Workers.
