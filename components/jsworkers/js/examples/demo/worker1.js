/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

function debug(aMsg) {
  postMessage(`[worker1.js] ${aMsg}`);
}

const dbName = "RemoteWorkerDB";

const customerData = [
  { ssn: "444-44-4444", name: "Bill", age: 35, email: "bill@company.com" },
  { ssn: "555-55-5555", name: "Donna", age: 32, email: "donna@home.org" }
];

function triggerError() {
  throw Error("worker error");
}

onmessage = function(event) {
  console.log(`Worker received ${event}`)
  if (event.data == "error") {
    triggerError();
    return;
  }
  if (event.data == "close") {
    close();
    return;
  }
  postMessage(`You send me: ${event.data}`);
}

var request = indexedDB.open(dbName, 2);

request.onerror = function(event) {
  debug("In onerror");
};

request.onsuccess = function(event) {
  debug("In onsuccess: indexedDB opened!");
};

request.onupgradeneeded = function(event) {
  debug("In onupgradeneeded");
  var db = event.target.result;

  var objectStore = db.createObjectStore("customers", { keyPath: "ssn" });

  objectStore.createIndex("name", "name", { unique: false });
  objectStore.createIndex("email", "email", { unique: true });

  objectStore.transaction.oncomplete = function(event) {
    var customerObjectStore = db.transaction("customers", "readwrite").objectStore("customers");
    for (var i in customerData) {
      customerObjectStore.add(customerData[i]);
      debug(`added ${JSON.stringify(customerData[i])} `);
    }
  };
};
