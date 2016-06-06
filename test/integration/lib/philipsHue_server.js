'use strict';

const express = require('express');
const bodyParser = require('body-parser');
const path = require('path');

var api_resp = require('./json/bridge_status.json');  // full status response
var philipsHue_server = (function() {

  // this represents the simulated lights
  var light_status; 
  var last_received_cmd = null;  
  var last_commanded_light; 
  var _server = express();
  var instance;
  var foxboxId;
  var server_port;
  var auth_status;

  function areAllLightsOn() {
    return [1, 2, 3].every(i => this.lightStatus(i));
  }

  function _setLight(id,field,value) { 
    light_status[id].state[field] = value;
  }

  function lastCmd() { 
    return last_received_cmd;
  }

  function lastLight() { 
    return last_commanded_light;
  }

  function lightResponse(light_id,result) {
    var response = {};
    var key = '/lights/' + light_id + '/state/on';
    response[key] = result;

    return [{'success': response}];
  }

  function turnOnLight(id) {
    _setLight(id,'on',true);
    console.log('light ' + id + ' status: ' + light_status[id].state.on);
  }

  function turnOffLight(id) {
    _setLight(id,'on',false);
    console.log('light ' + id + ' status: ' + light_status[id].state.on);
  }

  function turnOffAllLights(ids) {
    ids.forEach(id => {
      turnOffLight(id);
    });
  }

  function getHSB(id){
    var h,s,b;
    h = light_status[id].state.hue;
    s = light_status[id].state.sat;
    b = light_status[id].state.bri;
    return {'h':h,'s':s,'b':b};
  }

  function setHSB(id,h,s,b) {
    light_status[id].state.hue = h;
    light_status[id].state.sat = s;
    light_status[id].state.bri = b; 
  }

  function lightStatus(id) {
    return light_status[id].state.on;
  }

  // Equivalent to pressing the button on the Philips hub
  // issues the username to the requestor.
  function pressButton() {
    foxboxId = 'simulatedPhilipsHub';
    return new Promise(resolve => {
      setTimeout(resolve, 5000);
    });
  }

  function setup(port,authentication) {
    // Reset variables, since the instance gets 'reused' between tests
    light_status = api_resp.lights; 
    foxboxId = undefined;
    server_port = port;
    auth_status = authentication;

    return new Promise(resolve => {
     instance = _server.listen(server_port, function () {
      console.log('Hue simulator listening on port ' + server_port);
       resolve(); // it's like if you called `callback()`
     });
   });
  }

  function stop() {
   return new Promise(resolve => {
     instance.close(function(){
       console.log('philips hue server closed');
       resolve(); // it's like if you called `callback()`
     });
   });
  }
   
  // Callback methods

  _server.use(function (req, res, next) { 
    // foxbox does not have the json header, so need to 
    // inject it here so the message body can be properly captured
    req.headers['content-type'] = 'application/json';
    last_received_cmd = req.method + ':' + req.originalUrl;
    next();
  });

  _server.get('/', function (req, res) {
    res.status(200).sendFile(
      path.join(__dirname,'/html/philips_initial.html'));  
  });

  _server.get('/api/:foxboxId/', function (req, res) {   
      
     if (foxboxId === undefined) {
      if (auth_status === false) {
        foxboxId = req.params.foxboxId;
        res.sendStatus(200);  
      }
      // Althought it's an error message, Hue hub returns 200 OK
      else if (auth_status === true) {
        res.status(200).send([{
              'error': {
                  'type': 1,
                  'address': '/',
                  'description': 'unauthorized user'
              }
          }]);
      }     
    }
    else {
      res.status(200).json(api_resp);  
    }     
  });

  _server.post('/api', bodyParser.json(),function (req, res) {
    
    if (foxboxId === undefined) {
        res.status(200).send([{
            'error': {
                'type': 101,
                'address': '',
                'description': 'link button not pressed'
            }
        }]);
      }
      else {
        if (req.body.devicetype === 'foxbox_hub') {
          res.status(200).send([{
            'success': {
              'username': 'simulatedPhilipsHub'
            }
          }]);
        }
      }
  });

  _server.get('/api/:foxboxId/lights', function (req, res) {
    if (req.params.foxboxId === foxboxId){ 
      res.status(200).json(light_status); 
    }
    else {
      res.status(404); 
      throw 'foxboxId mismatch: got ' + req.params.foxboxId; 
    }
  });

  _server.get('/api/:foxboxId/lights/:lightId', function (req, res) {
    if (req.params.foxboxId === foxboxId){
      // return the appropriate subset of light status
      res.status(200).json(light_status[req.params.lightId]); 
    }
    else {
      res.status(404); 
      throw 'foxboxId mismatch: got ' + req.params.foxboxId; 
    }
  });

  _server.put('/api/:foxboxId/lights/:lightId/state', 
    bodyParser.json(),function (req, res) {
    if (req.body.hue !== undefined && 
      req.body.sat !== undefined && 
      req.body.bri !== undefined) {
      setHSB(req.params.lightId,req.body.hue,req.body.sat,req.body.bri);
    }
    
    if (req.body.on === true) {
      turnOnLight(req.params.lightId); 
    }
    else if (req.body.on === false) {
      turnOffLight(req.params.lightId);      
    }
    
    var response = lightResponse(req.params.lightId,
      light_status[req.params.lightId]);
    last_received_cmd += ':' + JSON.stringify(req.body);
    last_commanded_light = req.params.lightId;
    res.status(200).json(response);     
  });

  return {setup, stop, lastCmd, lastLight, getHSB, lightResponse, turnOnLight, 
    turnOffLight, turnOffAllLights, lightStatus, areAllLightsOn, 
    pressButton};

  })();

module.exports = philipsHue_server;