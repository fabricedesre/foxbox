'use strict';
const Config = require('config-js');

var config = new Config('./test/integration/lib/config/foxbox.js');
var philipshue_server = require('../lib/philipsHue_server.js');
var ipcamera_server = require('../lib/ipcamera_server.js');
var webPush_server = require('../lib/webpush_server.js');
var nupnp_server = require('../lib/nupnp_PhilipsHue.js');
var foxboxManager = require('../lib/foxbox_process_manager.js');
var config = new Config('./test/integration/lib/config/foxbox.js');


var make_suite = (function() {
  var nupnpSimulatorOn = false;
  var hueSimulatorOn = false;
  var cameraSimulatorOn = false;
  var webPushSimulatorOn = false;
  var foxboxOn = false;
  
  function turnOnAllSimulators() { 

    // Note: when one of the simulators needs to be enabled later, make sure
    // it is in the before hook with the turnOnxxx method returning the promise
    var promises = [];

    promises.push(turnOnHueNupnp());
    promises.push(turnOnHue());
    promises.push(turnOnCamera());
    promises.push(turnOnWebPush());
    
    return Promise.all(promises);
 }

  function turnOnHueNupnp() {
      nupnpSimulatorOn = true;
      return nupnp_server.start(config.get('nupnp_server.id'),
      config.get('philips_hue.url') +':'+config.get('philips_hue.port'),
      config.get('nupnp_server.port'));
  }

  function turnOnHue(authentication){
    authentication = authentication || false;

    hueSimulatorOn = true;
    return philipshue_server.setup(
      config.get('philips_hue.port'),authentication);
  }

  function turnOnCamera() {
    cameraSimulatorOn = true;
    return ipcamera_server.setup();
  }

  function turnOnWebPush() {
    webPushSimulatorOn = true;
    return webPush_server.setup();
  }

  function turnOnFoxbox() {
    foxboxOn = true;
    return foxboxManager.fullOptionStart();
  }

  function nupnpOff() {
   nupnpSimulatorOn = false;
    return nupnp_server.stop();
  }

  function hueOff() {
    hueSimulatorOn = false;
    return philipshue_server.stop();
  }

  function cameraOff() {
    cameraSimulatorOn = false;
    return ipcamera_server.stop();
  }

  function webPushOff() {
    webPushSimulatorOn = false;
    return webPush_server.stop();
  }

  function makeSuite(desc, subSuite) {
    describe(desc, function () {
      this.timeout(60000);
      subSuite();
      after(function() {
        var promises = [];
        foxboxManager.killFoxBox();
        foxboxManager.cleanData();

        if (hueSimulatorOn){
          promises.push(nupnpOff());
          promises.push(hueOff());
        }
        if (cameraSimulatorOn) {
          promises.push(cameraOff());
        }
        if (webPushSimulatorOn) {
          promises.push(webPushOff());
        }
        return Promise.all(promises);
      });
    });
  }

  return { makeSuite, turnOnAllSimulators,
    philipshue_server,ipcamera_server, webPush_server,
    foxboxManager, turnOnFoxbox, turnOnHue, turnOnHueNupnp};
})();

module.exports = make_suite;