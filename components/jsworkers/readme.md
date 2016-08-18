# Overview

## Lifecycle and book keeping.
Gecko doesn't do any specific book keeping, but at the foxbox level the behavior depends on the worker's kind:

- Web workers: a web worker is kept alive by the refcount of clients that started it and didn't terminate() it. If the worker itself calls close() it shuts down all clients. There is no need to restart web workers at startup on the foxbox or jsrunner side.

- Service workers: these follow the register/unregister cycle. Registering twice the same SW is idempotent, thus foxbox can register them at startup so that gecko doesn't have to do any special book keeping.

## Public urls & SW access

- The page asks to register https://mysite.com/path/sw.js with a scope of `/path`
- Gecko loads the wrapper https://mysite.com/$UUID/serviceworkerwrapper.html that registers the SW from its original URI with a scope of `$sw_path/$UUID/path`.
- foxbox registers that the $foxbox_host/user/$user_id/mysite.com/ URLs will be managed by this SW.

- when a request comes in for $foxbox_host/user/$user_id/mysite.com/some/file.html the foxbox sends a request to gecko for ($user_id, mysite.com/some/file.html)
- gecko fetches (using fetch()) the resource at https://mysite.com/$UUID/some/file.html which is intercepted by the installed service worker and returns the response to foxbox.

Service workers scopes are rooted at $domain/user/$user_id/ which can be considered as a "virtual base" for each user.

# TODO

## Foxbox:
- Turn into an adapter?
- More tests.

## Gecko:
- Create the worker with a per-usercontext principal.
- Worker cache policy?
- Tests.

## Web library
- Authentication.

## All:
- http mode.

## Service Workers
- URL routing.
- Registering should return a promise.
- Unregistration.
