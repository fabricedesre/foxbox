'use strict';
const fs = require('fs');
const find = require('find');
const path = require('path');
const Config = require('config-js');
const chakram = require('chakram'), expect = chakram.expect;

const spawn = require('child_process').spawn;
const PROFILE_PATH = path.join(process.env.HOME, '.local/share/foxbox/');

var config = new Config('./test/integration/lib/config/foxbox.js');
var FOXBOX_STARTUP_WAIT_TIME_IN_MS = 5000;
var foxboxInstance;

var foxbox_process_manager = (function() {

  var setupURL = config.get('foxbox.url') + '/users/setup';
  var loginURL = config.get('foxbox.url') + '/users/login';
  var serviceURL = config.get('foxbox.url') + '/api/v1';
  var serviceListURL = serviceURL + '/services'; 
  var getterURL = serviceURL + '/channels/get';
  var setterURL = serviceURL + '/channels/set';

  function cleanData() {
    var files = find.fileSync(/^((?!\.pem).)*$/, PROFILE_PATH);
    files.forEach(function(entry) {
      fs.unlinkSync(entry);
    });  
  }

  function fullOptionStart() {
    foxboxInstance = spawn('./target/debug/foxbox',
      ['-c',  config.get('nupnp_server.param')+';'+
      config.get('nupnp_server.url')+':'+
      config.get('nupnp_server.port')+'/',
      '--disable-tls'], {stdio: 'inherit'} ); // TODO TLS not yet supported

    return new Promise(resolve => {
      setTimeout(resolve, FOXBOX_STARTUP_WAIT_TIME_IN_MS);
    });
  }

  function foxboxLogin() {
    var credential = config.get('credential'); 
    var header = new Config('./test/integration/lib/config/header.js');

    return chakram.post(setupURL,credential)
    .then(function(setupResp){
      expect(setupResp).to.have.status(201);
      var encoded_cred = new Buffer(credential.username+
      ':'+credential.password).toString('base64');
      header.Authorization = 'Basic ' + encoded_cred;

      return chakram.post(loginURL,null,{'headers' : header});
    })
    .then(function(resp){
       header.Authorization = 'Bearer ' + resp.body.session_token;
       chakram.setRequestDefaults({'headers': header});
     });
  }

  function killFoxBox() {
    foxboxInstance.kill('SIGINT');
  }

  function getLatestIPFromPingSrv(body) {
    var pick;
    var timestamp = 0;

    for (var match in body) {
      // may be multiple entries.  in that case, pick latest
      if (parseInt(body[match].timestamp) > 
        parseInt(timestamp)) {
        timestamp = body[match].timestamp;
        pick = match;
      }
    }
    return pick;
  }

  return {setupURL, loginURL, serviceURL, serviceListURL, getterURL, setterURL,
    cleanData, fullOptionStart, foxboxLogin, killFoxBox, 
    getLatestIPFromPingSrv};
})();

module.exports = foxbox_process_manager;
