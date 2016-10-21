/*jshint eqeqeq:false */
(function (window) {
	'use strict';

	FoxboxWorkers.use_remote(true);
	var worker = FoxboxWorkers.Worker("http://jsworkers.org:8000/examples/todomvc/js/worker.js");
	var messageId = 0;

	// Sends a message to the worker, and transmits the matching response to the
	// callback.
	function sendToWorker(name, message, callback) {
		callback = callback || function () {};

		messageId += 1;
		var id = messageId;

		worker.addEventListener("message", function workerMessage(event) {
			var data = event.data;
			if (data.id == id) {
				console.log(`Got result for ${id}`);
				worker.removeEventListener("message", workerMessage);
				callback.call(this, data.result);
			}
		});

		console.log(`Sending ${name} ${id}`);
		worker.postMessage({ id: id, name: name, content: message });
	}

	/**
	 * Creates a new client side storage object and will create an empty
	 * collection if no collection already exists.
	 *
	 * @param {string} name The name of our DB we want to use
	 * @param {function} callback Our fake DB uses callbacks because in
	 * real life you probably would be making AJAX calls
	 */
	function Store(name, callback) {
		sendToWorker("create", { name: name }, callback);
	}

	/**
	 * Finds items based on a query given as a JS object
	 *
	 * @param {object} query The query to match against (i.e. {foo: 'bar'})
	 * @param {function} callback	 The callback to fire when the query has
	 * completed running
	 *
	 * @example
	 * db.find({foo: 'bar', hello: 'world'}, function (data) {
	 *	 // data will return any items that have foo: bar and
	 *	 // hello: world in their properties
	 * });
	 */
	Store.prototype.find = function (query, callback) {
		sendToWorker("find", { query: query }, callback);
	};

	/**
	 * Will retrieve all data from the collection
	 *
	 * @param {function} callback The callback to fire upon retrieving data
	 */
	Store.prototype.findAll = function (callback) {
		sendToWorker("findAll", [], callback);
	};

	/**
	 * Will save the given data to the DB. If no item exists it will create a new
	 * item, otherwise it'll simply update an existing item's properties
	 *
	 * @param {object} updateData The data to save back into the DB
	 * @param {function} callback The callback to fire after saving
	 * @param {number} id An optional param to enter an ID of an item to update
	 */
	Store.prototype.save = function (updateData, callback, id) {
		sendToWorker("save", { updateData: updateData, id: id }, callback);
	};

	/**
	 * Will remove an item from the Store based on its ID
	 *
	 * @param {number} id The ID of the item you want to remove
	 * @param {function} callback The callback to fire after saving
	 */
	Store.prototype.remove = function (id, callback) {
		sendToWorker("remove", { id: id }, callback);
	};

	/**
	 * Will drop all storage and start fresh
	 *
	 * @param {function} callback The callback to fire after dropping the data
	 */
	Store.prototype.drop = function (callback) {
		sendToWorker("drop", [], callback);
	};

	// Export to window
	window.app = window.app || {};
	window.app.Store = Store;
})(window);
