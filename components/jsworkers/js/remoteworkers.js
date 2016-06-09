/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this file,
 * You can obtain one at http://mozilla.org/MPL/2.0/. */

(function(global) {
  var BOX_BASE_URL = null;
  var MODE = "local";

  function RemoteWorker(worker_url) {
    console.log(`Starting remote worker at ${worker_url}`);
    // Creates a worker client object. This needs to be sync so the returned
    // object is in pending state at this stage, waiting for the http request
    // to complete and then the websocket connection to be established.
    // state can be: "pending_http", "pending_ws", "ready" or "error".
    this.state = "pending_http";
    this.url = worker_url;
    this.send_http_request()
        .then(this.on_http_response.bind(this))
        .then(this.open_ws_connection.bind(this))
        .catch(error => {
          console.error(error);
          this.state = "error";
        });
  }

  // TODO: add a shim EventTarget implementation.
  RemoteWorker.prototype = {
    expect_state: function(state) {
      if (this.state != state) {
        throw new Error(`Expected state to be ${state} but found ${this.state}`);
      }
    },

    // Send the initial http request.
    send_http_request: function() {
      this.expect_state("pending_http");
      let url = BOX_BASE_URL + "/jsworkers/v1/start";
      let init = {
        method: "POST",
        body: JSON.stringify({ webworker_url: this.url }),
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
            self.state = "pending_ws";
            resolve(json.ws_url);
          } else {
            reject("NoWsUrl");
          }
        }).catch((e) => reject(e));
      });
    },

    open_ws_connection: function(ws_url) {
      this.expect_state("pending_ws");
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
          this.state = "error";
          break;
        case "open":
          console.log(`Websocket opened for ${this.url}`);
          this.state = "ready";
          break;
        case "message":
          console.log(`Message received for ${this.url} : ${event.data} size is ${event.data.size}`);
          if (this.onmessage && typeof this.onmessage === "function") {
            var self = this;
            var reader = new FileReader();
            reader.addEventListener("loadend", function() {
              self.onmessage(window.ObjectEncoder.decode(reader.result));
            });
            reader.readAsArrayBuffer(event.data);
          }
          break;
        default:
          console.error(`Unexpected event type: ${event.type}`);
      }
    },

    postMessage: function(message) {
      // TODO: should we buffer the messages until we're ready?
      this.expect_state("ready");
      window.ObjectEncoder.encode(message).then(encoded => {
        this.ws.send(encoded);
      });
    }
  }

  var FoxboxWorkers = {
    // Sets the base url of the box, eg. http://localhost:3000
    set_base_url: function(url) {
      // TODO: check that `url` is actually a url.
      BOX_BASE_URL = url;
      MODE = "remote";
    },

    start: function(worker_url) {
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
