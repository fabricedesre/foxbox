'use strict';

const chakram = require('chakram'), expect = chakram.expect;
const Config = require('config-js');

var config = new Config('./test/integration/lib/config/foxbox.js');
var Prepper = require('../../lib/make_suite.js');

Prepper.makeSuite('Verify validity of pagekite ping endpoint', function(){
  var local_url;
  var credential = config.get('credential'); 
  var pingUrl = config.get('pagekite.r') + '/ping';
  
  before(Prepper.turnOnAllSimulators);
  before(Prepper.turnOnFoxbox);

  it('get address from pagekite ping endpoint', function() {  
    var pick;
    var timestamp = 0;

    return  chakram.get(pingUrl,{'json': true})
    .then(function(pingResp){
      for (var match in pingResp.body) {
        // may be multiple entries.  in that case, pick latest
        if (parseInt(pingResp.body[match].timestamp) > 
          parseInt(timestamp)) {
          timestamp = pingResp.body[match].timestamp;
          pick = match;
        }
    }
    
    expect(pingResp).to.have.status(200);
    expect(pingResp.body[pick].timestamp).to.match(/\d+/);
    expect(pingResp.body[pick].public_ip).to.match(/\d+.\d+.\d+.\d+/);
    // Since tls is disabled, there is no tunnel address
    expect(pingResp.body[pick].tunnel_origin).equals(undefined);  
    });
  });
  
  describe('Initiate the remote connection',function(){
    before('fetch the url', function() {
      return  chakram.get(pingUrl,{'json': true})
      .then(function(pingResp){
        var entry = Prepper.foxboxManager.getLatestIPFromPingSrv(pingResp.body);
        var originList = JSON.parse(pingResp.body[entry].message);
      
        local_url = originList.local_origin;
      });
    });

    it('create account via local_origin',function(){   
      return chakram.post(local_url + '/users/v1/setup', credential)
      .then(function(setupResp) {
        expect(setupResp).to.have.status(201);
      });
    });
  });
});
