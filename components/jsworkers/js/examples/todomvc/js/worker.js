// Worker based store for todomvc.
// Very basic implementation, that both keeps an in-memory version and a
// persisted one.

importScripts("./async_storage.js");

function log(msg) {
  console.log(`WorkerStore: ${msg}`);
  dump(`\x1b[1;34m[WorkerStore] ${msg}\x1b[0m\n`);
}

log(`asyncStorage is ${asyncStorage}`);

// Receives messages fromr the main thread.
// event.data is : { name: "create|find|findAll|save|remove|drop", id: "messageId", content: "..." }
self.onmessage = function(event) {
  var id = event.data.id;
  if (event.data.name == "create") {
    create(event.data.content, id);
  } else {
    createdPromise.then(() => {
      switch (event.data.name) {
        case "find":
          find(event.data.content, id);
          break;
        case "findAll":
          findAll(id);
          break;
        case "save":
          save(event.data.content, id);
          break;
        case "remove":
          remove(event.data.content, id);
          break;
        case "drop":
          drop(event.data.content, id);
          break;
      }
    });
  }
}

var db;
var keyName;

var deferredPromise = {};
var createdPromise = new Promise((resolve) => {
  deferredPromise.resolve = resolve;
});

function create(content, id) {
  log(`Create store ${content.name}`);
  keyName = `todo-store-${content.name}`;
  asyncStorage.getItem(keyName, (result) => {
    if (result !== null) {
      db = result;
    } else {
      db = { todos: [] };
    }
    postMessage({ id: id, result: result });
    deferredPromise.resolve();
    log(`promise resolved, db is ${db}`);
  });
}

function find(content, id) {
  log(`Find in store ${JSON.stringify(content.query)}`);
  var todos = db.todos;
  var result = todos.filter((todo) => {
    for (var q in content.query) {
      if (content.query[q] !== todo[q]) {
        return false;
      }
    }
    return true;
  });
  postMessage({ id: id, result: result });
}

function findAll(id) {
  log(`Findall in store`);
  postMessage({ id: id, result: db.todos });
}

function save(content, id) {
  log(`Save in store ${JSON.stringify(content.updateData)}`);

  var todos = db.todos;
  // If an ID was actually given, find the item and update each property
  if (content.id) {
    for (var i = 0; i < todos.length; i++) {
      if (todos[i].id === content.id) {
        for (var key in content.updateData) {
          todos[i][key] = content.updateData[key];
        }
        break;
      }
    }

    asyncStorage.setItem(keyName, db, () => {
      postMessage({ id: id, result: db.todos });
    });
  } else {
    // Generate an ID
    content.updateData.id = new Date().getTime();

    todos.push(content.updateData);
    asyncStorage.setItem(keyName, db, () => {
      postMessage({ id: id, result: [content.updateData] });
    });
  }
}

function remove(content, id) {
  log(`Remove from store ${content.id}`);
  var todos = db.todos;

  for (var i = 0; i < todos.length; i++) {
    if (todos[i].id == content.id) {
      todos.splice(i, 1);
      break;
    }
  }
  asyncStorage.setItem(keyName, db, () => {
    postMessage({ id: id, result: db.todos });
  });
}

function drop(id) {
  log(`Drop store`);
  db = { todos: [] };
  asyncStorage.setItem(keyName, db, () => {
    postMessage({ id: id, result: db.todos });
  });
}
