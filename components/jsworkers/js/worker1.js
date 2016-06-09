function debug(aMsg) {
  postMessage(`[worker1.js] ${aMsg}`);
}

const dbName = "RemoteWorkerDB";

const customerData = [
  { ssn: "444-44-4444", name: "Bill", age: 35, email: "bill@company.com" },
  { ssn: "555-55-5555", name: "Donna", age: 32, email: "donna@home.org" }
];

var request = indexedDB.open(dbName, 2);

request.onerror = function(event) {
  debug("In onerror");
};

request.onsuccess = function(event) {
  debug("In onsuccess");
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
