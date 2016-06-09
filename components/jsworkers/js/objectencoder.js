/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this file,
 * You can obtain one at http://mozilla.org/MPL/2.0/. */

// Encoder & decoder to exchange JS objects as ArrayBuffers.
// This doesn't support cyclic references, and makes no effort at being compact.

"use strict";

(function(global) {

function debug(aMsg) {
  console.log("-*- ObjectEncoder: " + aMsg + "\n");
}

const kTypeFlags = {
  "null"             : 0x00,
  "undefined"        : 0x01,
  "array"            : 0x02,
  "blob"             : 0x04,
  "date"             : 0x05,
  "object"           : 0x06,
  "Int8Array"        : 0x07,
  "Uint8Array"       : 0x08,
  "Uint8ClampedArray": 0x09,
  "Int16Array"       : 0x0a,
  "Uint16Array"      : 0x0b,
  "Int32Array"       : 0x0c,
  "Uint32Array"      : 0x0d,
  "Float32Array"     : 0x0e,
  "Float64Array"     : 0x0f,
  "string"           : 0x10,
  "number"           : 0x11,
  "boolean"          : 0x12,
};

const kTypedArrays = new Set([
  "Int8Array",
  "Uint8Array",
  "Uint8ClampedArray",
  "Int16Array",
  "Uint16Array",
  "Int32Array",
  "Uint32Array",
  "Float32Array",
  "Float64Array",
]);

const kPrimitiveTypes = new Set([
  "string",
  "boolean",
  "number",
  "object",
]);

function getObjectKind(aObject) {
  if (aObject === null) {
    return "null";
  } else if (aObject === undefined) {
    return "undefined";
  } else if (Array.isArray(aObject)) {
    return "array";
  } else if (aObject.constructor.name == "Blob") {
    return "blob";
  } else if (aObject.constructor.name == "Date") {
    return "date";
  } else if (kTypedArrays.has(aObject.constructor.name)) {
    return aObject.constructor.name;
  } else {
    let type = typeof aObject;
    return kPrimitiveTypes.has(type) ? type : null;
  }
}

const encoder = new TextEncoder("utf-8");
const decoder = new TextDecoder("utf-8");

function getStringSize(aString) {
  return 4 + encoder.encode(aString).byteLength;
}

function getInnerStringSize(aString) {
  return encoder.encode(aString).byteLength;
}

function encode32(aNumber, aBuffer, aOffset) {
  debug("encode32 " + aNumber + " offset " + aOffset);
  let view = new Uint8Array(aBuffer, aOffset);
  view[3] = aNumber & 0xff;
  view[2] = (aNumber >> 8) & 0xff;
  view[1] = (aNumber >> 16) & 0xff;
  view[0] = (aNumber >> 24) & 0xff;
}

function decode32(aBuffer, aOffset) {
  debug("decode32 offset " + aOffset);
  let res = 0;
  let view = new Uint8Array(aBuffer, aOffset);
  for (let i = 0; i < 4; i++) {
    res = (res << 8) | view[i];
  }
  return res;
}

function findKindFromFlag(aFlag) {
  debug("findKindFromFlag " + aFlag);
  for (let prop in kTypeFlags) {
    if (kTypeFlags[prop] === aFlag) {
      return prop;
    }
  }
  return null;
}

global.ObjectEncoder = {
  _encodenull: function(aObject, aBuffer, aOffset) {
    debug("_encodenull offset " + aOffset);
    return Promise.resolve(aOffset);
  },

  _encodeundefined: function(aObject, aBuffer, aOffset) {
    debug("_encodeundefined offset " + aOffset);
    return Promise.resolve(aOffset);
  },

  _encodestring: function(aString, aBuffer, aOffset) {
    let size = getInnerStringSize(aString);
    debug(`_encodestring '${aString}' size ${size} offset ${aOffset}`);
    encode32(size, aBuffer, aOffset);
    aOffset += 4;
    let stringArray = encoder.encode(aString);
    let view = new Uint8Array(aBuffer, aOffset);
    view.set(stringArray);
    aOffset += size;
    return Promise.resolve(aOffset);
  },

  _encodenumber: function(aNumber, aBuffer, aOffset) {
    debug("_encodenumber offset " + aOffset);
    return this._encodestring("" + aNumber, aBuffer, aOffset);
  },

  _encodedate: function(aDate, aBuffer, aOffset) {
    debug("_encodedate offset " + aOffset);
    return this._encodestring(aDate.toISOString(), aBuffer, aOffset);
  },

  _encodeboolean: function(aBoolean, aBuffer, aOffset) {
    debug("_encodeboolean offset " + aOffset);
    let view = new Uint8Array(aBuffer, aOffset);
    view[0] = aBoolean ? 1 : 0;
    aOffset++;
    return Promise.resolve(aOffset);
  },

  _encodeobject: function(aObject, aBuffer, aOffset) {
    debug(`_encodeobject offset ${aOffset}`);

    // Encode the object property count.
    let props = Object.getOwnPropertyNames(aObject);
    encode32(props.length, aBuffer, aOffset);
    aOffset += 4;

    // Build a set of promises for each property, with the right offset.
    let promises = [];
    for (let prop in aObject) {
      let item = aObject[prop];
      let size = getStringSize(prop) + this.sizeOf(item);
      let offset = aOffset;
      let p = new Promise((aResolve, aReject) => {
        // First encode the property name, then the value.
        debug("Promise with o1=" + offset + " o2=" + (offset + getStringSize(prop)));
        this._encodestring(prop, aBuffer, offset)
          .then(newOffset => { return this._encode(item, aBuffer, newOffset); })
          .then(aResolve, aReject);
      });
      promises.push(p);
      aOffset += size;
    }

    return new Promise((aResolve, aReject) => {
      Promise.all(promises).then(() => { aResolve(aOffset); }, aReject);
    });
  },

  _encodearray: function(aArray, aBuffer, aOffset) {
    debug(`_encodearray offset ${aOffset}`);

    // Encode the array length.
    encode32(aArray.length, aBuffer, aOffset);
    aOffset += 4;

    // Build a set of promises for each item, with the right offset.
    let promises = [];
    aArray.forEach(aItem => {
      let size = this.sizeOf(aItem);
      let offset = aOffset;
      let p = this._encode(aItem, aBuffer, offset);
      promises.push(p);
      aOffset += size;
    }, this);

    return new Promise((aResolve, aReject) => {
      Promise.all(promises).then(() => { aResolve(aOffset); }, aReject);
    });
  },

  _encode: function(aObject, aBuffer, aOffset) {
    let kind = getObjectKind(aObject);
    debug("_encode kind=" + kind + " at offset " + aOffset);

    if (!kind || !(kind in kTypeFlags)) {
      return Promise.reject("Unknown or unsupported object kind");
    }

    let encoderName = "_encode" + kind;
    if (!(encoderName in this) || !(typeof this[encoderName] == "function")) {
      debug("unsupported: " + kind);
      return Promise.reject("Unable to encode: " + kind);
    }

    let view = new Uint8Array(aBuffer, aOffset);
    view[0] = kTypeFlags[kind];
    aOffset++;

    return this[encoderName](aObject, aBuffer, aOffset);
  },

  // Encodes an object, returns a promise resolving to an ArrayBuffer.
  encode: function(aObject, aPrefix) {
    let size = this.sizeOf(aObject);
    let hasPrefix = aPrefix !== undefined;
    if (hasPrefix) {
      size++;
    }

    debug("encoding object, size is " + size);
    let buffer = new ArrayBuffer(size);

    if (hasPrefix) {
      let view = new Uint8Array(buffer);
      view[0] = aPrefix;
    }

    return new Promise((aResolve, aReject) => {
      this._encode(aObject, buffer, hasPrefix ? 1 : 0 /* offset */)
        .then(() => { aResolve(buffer) }, aReject);
    });
  },

  _decodestring: function(aBuffer, aOffset) {
    let size = decode32(aBuffer, aOffset);
    debug("_decodestring size is " + size + " offset " + aOffset);
    aOffset += 4;
    let view = new Uint8Array(aBuffer, aOffset, size);
    aOffset += size;
    let s = decoder.decode(view);
    return [s, aOffset];
  },

  _decodenumber: function(aBuffer, aOffset) {
    debug("_decodenumber offset " + aOffset);
    let [s, offset] = this._decodestring(aBuffer, aOffset);
    return [Number(s), offset];
  },

  _decodedate: function(aBuffer, aOffset) {
    debug("_decodedate offset " + aOffset);
    let [s, offset] = this._decodestring(aBuffer, aOffset);
    debug("_decodedate " + s);
    return [new Date(s), offset];
  },

  _decodeundefined: function(aBuffer, aOffset) {
    debug("_decodeundefined offset " + aOffset);
    return [undefined, aOffset];
  },

  _decodenull: function(aBuffer, aOffset) {
    debug("_decodenull offset " + aOffset);
    return [null, aOffset];
  },

  _decodeboolean: function(aBuffer, aOffset) {
    debug("_decodeboolean offset " + aOffset);
    let view = new Uint8Array(aBuffer, aOffset);
    return [view[0] == 1, aOffset + 1];
  },

  _decodeobject: function(aBuffer, aOffset) {
    debug("_decodeobject " + aOffset);

    let propCount = decode32(aBuffer, aOffset);
    aOffset += 4;
    debug("object has " + propCount + " properties");
    let obj = {};
    for (let i = 0; i < propCount; i++) {
      let [name, offset] = this._decodestring(aBuffer, aOffset);
      debug("found prop: " + name);
      let [value, offset2] = this._decode(aBuffer, offset);
      aOffset = offset2;
      obj[name] = value;
    }
    return [obj, aOffset];
  },

  _decodearray: function(aBuffer, aOffset) {
    debug("_decodearray " + aOffset);

    let size = decode32(aBuffer, aOffset);
    aOffset += 4;
    debug("array has " + size + " items");
    let array = [];
    for (let i = 0; i < size; i++) {
      let [value, offset] = this._decode(aBuffer, aOffset);
      aOffset = offset;
      array.push(value);
    }
    return [array, aOffset];
  },

  _decode: function(aBuffer, aOffset) {
    debug("_decode " + aOffset);
    let view = new Uint8Array(aBuffer, aOffset);

    let kind = findKindFromFlag(view[0]);
    let decoderName = "_decode" + kind;
    debug("decoding " + kind);

    if (!(decoderName in this) || !(typeof this[decoderName] == "function")) {
      debug("unsupported: " + kind);
      throw "Unimplemented";
    }
    return this[decoderName](aBuffer, aOffset + 1);
  },

  decode: function(aBuffer, aInitialOffset) {
    let size = aBuffer.byteLength;
    debug(`decoding ${aBuffer}, size is ${size}`);
    let [obj, newOffset] = this._decode(aBuffer, aInitialOffset || 0);

    return obj;
  },

  // returns the size in bytes that we need to store this object.
  sizeOf: function(aObject) {
    let kind = getObjectKind(aObject);
    if (!kind) {
      return 0;
    }

    let size;
    switch(kind) {
      case "null":
      case "undefined":
        size = 0;
        break;
      case "array":
        size = 4; // 32 bits array length.
        aObject.forEach(aItem => { size += this.sizeOf(aItem); }, this);
        break;
      case "blob":
        // total size + blob size + type size.
        size = 4 + aObject.size + getStringSize(aObject.type);
        break;
      case "date":
        size = getStringSize(aObject.toISOString());
        break;
      case "object":
        size = 4; // 32 bits property count.
        for (let prop in aObject) {
          size += getStringSize(prop) + this.sizeOf(aObject[prop]);
        }
        break;
      case "string":
        size = getStringSize(aObject);
        break;
      case "boolean":
        size = 1;
        break;
      case "number":
        size = getStringSize("" + aObject);
        break;
      default: // all the ArrayBufferView types.
        if (kTypedArrays.has(kind)) {
          size = 4 + aObject.byteLength;
        } else {
          debug("Unknown object kind: " + kind + "\n");
        }
        break;
    }

    return size + 1; // Adding 1 for the flag size.
  }
};
})(window);
