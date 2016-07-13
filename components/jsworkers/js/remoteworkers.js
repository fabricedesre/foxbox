/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this file,
 * You can obtain one at http://mozilla.org/MPL/2.0/. */

(function(global) {
  var BOX_BASE_URL = null;
  // When mode is "local" we spawn an in-browser web worker.
  var MODE = "local";

  // Global state values.
  const kPendingHttp = 0;
  const kPendingWs   = 1;
  const kReady       = 2;
  const kError       = 3;

  // WS message event states.
  const kCommand = 0;
  const kPayload = 1;

  function RemoteWorker(worker_url) {
    console.log(`Starting remote worker at ${worker_url}`);
    // Creates a worker client object. This needs to be sync so the returned
    // object is in pending state at this stage, waiting for the http request
    // to complete and then the websocket connection to be established.
    this.state = kPendingHttp;
    this.url = worker_url;

    // The state of the onmessage ws handler. We process pair of messages, the
    // fist one being a command ('message' or 'error') and the second one being
    // the payload.
    this.ws_state = kCommand;
    this.ws_command = null;

    this.send_http_request()
        .then(this.on_http_response.bind(this))
        .then(this.open_ws_connection.bind(this))
        .catch(error => {
          console.error(error);
          this.state = kError;
        });
  }

  RemoteWorker.prototype = {
    // Start of EventTarget shim from https://cs.chromium.org/chromium/src/ui/webui/resources/js/cr/event_target.js
    // Copyright (c) 2010 The Chromium Authors. All rights reserved.
    // Use of this source code is governed by a BSD-style license that can be
    // found in the LICENSE file.
    /**
     * Adds an event listener to the target.
     * @param {string} type The name of the event.
     * @param {EventListenerType} handler The handler for the event. This is
     *     called when the event is dispatched.
     */
    addEventListener: function(type, handler) {
      if (!this.listeners_)
        this.listeners_ = Object.create(null);
      if (!(type in this.listeners_)) {
        this.listeners_[type] = [handler];
      } else {
        var handlers = this.listeners_[type];
        if (handlers.indexOf(handler) < 0)
          handlers.push(handler);
      }
    },
    /**
     * Removes an event listener from the target.
     * @param {string} type The name of the event.
     * @param {EventListenerType} handler The handler for the event.
     */
    removeEventListener: function(type, handler) {
      if (!this.listeners_)
        return;
      if (type in this.listeners_) {
        var handlers = this.listeners_[type];
        var index = handlers.indexOf(handler);
        if (index >= 0) {
          // Clean up if this was the last listener.
          if (handlers.length == 1)
            delete this.listeners_[type];
          else
            handlers.splice(index, 1);
        }
      }
    },
    /**
     * Dispatches an event and calls all the listeners that are listening to
     * the type of the event.
     * @param {!Event} event The event to dispatch.
     * @return {boolean} Whether the default action was prevented. If someone
     *     calls preventDefault on the event object then this returns false.
     */
    dispatchEvent: function(event) {
      if (!this.listeners_)
        return true;
      // Since we are using DOM Event objects we need to override some of the
      // properties and methods so that we can emulate this correctly.
      var self = this;
      event.__defineGetter__('target', function() {
        return self;
      });
      var type = event.type;
      var prevented = 0;
      if (type in this.listeners_) {
        // Clone to prevent removal during dispatch
        var handlers = this.listeners_[type].concat();
        for (var i = 0, handler; handler = handlers[i]; i++) {
          if (handler.handleEvent)
            prevented |= handler.handleEvent.call(handler, event) === false;
          else
            prevented |= handler.call(this, event) === false;
        }
      }
      return !prevented && !event.defaultPrevented;
    },
    // End of EventTarget shim from https://cs.chromium.org/chromium/src/ui/webui/resources/js/cr/event_target.js

    expect_state: function(state) {
      if (this.state != state) {
        throw new Error(`Expected state to be ${state} but found ${this.state}`);
      }
    },

    // Send the initial http request.
    send_http_request: function() {
      this.expect_state(kPendingHttp);
      let url = BOX_BASE_URL + "/jsworkers/v1/start";
      let init = {
        method: "POST",
        body: JSON.stringify({ url: this.url }),
        mode: "cors"
      }
      return global.fetch(url, init);
    },

    // Processes the http response.
    on_http_response: function(response) {
      if (!response.ok) {
        return Promise.reject("InvalidResponse");
      }

      let self = this;
      return new Promise((resolve, reject) => {
        response.json().then(function(json) {
          if (json.ws_url) {
            // TODO: check that this is actually a url.
            self.state = kPendingWs;
            resolve(json.ws_url);
          } else {
            reject("NoWsUrl");
          }
        }).catch((e) => reject(e));
      });
    },

    open_ws_connection: function(ws_url) {
      this.expect_state(kPendingWs);
      console.log(`Opening websocket connection to ${ws_url}`);
      this.ws = new global.WebSocket(ws_url);

      this.ws.addEventListener("close", this);
      this.ws.addEventListener("error", this);
      this.ws.addEventListener("open", this);
      this.ws.addEventListener("message", this);
    },

    // Handle the websocket events.
    handleEvent: function(event) {
      switch(event.type) {
        case "error":
        case "close":
          // TODO: investigate if we can recover from a remote closure.
          this.state = kError;
          break;
        case "open":
          console.log(`Websocket opened for ${this.url}`);
          this.state = kReady;
          break;
        case "message":
          console.log(`Message received for ${this.url} : ${event.data}`);
          if (this.ws_state === kCommand) {
            this.ws_command = event.data;
            this.ws_state = kPayload;
            return;
          }

          if (this.ws_state !== kPayload) {
            console.error(`Unexpected ws state: ${this.ws_state}`);
            return;
          }

          var self = this;
          var reader = new FileReader();
          reader.addEventListener("loadend", function() {
            let decoded = window.ObjectEncoder.decode(reader.result);
            if (self.ws_command === "message") {
              if (self.onmessage && typeof self.onmessage === "function") {
                self.onmessage(decoded);
              }
              self.dispatchEvent({ type: "message", data: decoded });
            } else if (self.ws_command == "error") {
              if (self.onerror && typeof self.onerror === "function") {
                self.onerror(new ErrorEvent("error", decoded));
              }
              self.dispatchEvent(new ErrorEvent("error", decoded));
            }
          });
          reader.readAsArrayBuffer(event.data);
          this.ws_state = kCommand;
          break;
        default:
          console.error(`Unexpected event type: ${event.type}`);
      }
    },

    postMessage: function(message) {
      // TODO: should we buffer the messages until we're ready?
      this.expect_state(kReady);
      window.ObjectEncoder.encode(message).then(encoded => {
        this.ws.send(encoded);
      });
    },

    terminate: function() {
      this.expect_state(kReady);
      this.ws.close();
      this.state = kError;
    }
  }

  var FoxboxWorkers = {
    // Sets the base url of the box, eg. http://localhost:3000
    set_base_url: function(url) {
      // TODO: check that `url` is actually a url.
      BOX_BASE_URL = url;
      MODE = "remote";
    },

    Worker: function(worker_url) {
      if (MODE == "local") {
        console.log(`Starting local worker at ${worker_url}`);
        return new global.Worker(worker_url);
      } else {
        return new RemoteWorker(worker_url);
      }
    }
  }

  console.log("Exporting FoxboxWorkers");
  global.FoxboxWorkers = FoxboxWorkers;
})(window);
