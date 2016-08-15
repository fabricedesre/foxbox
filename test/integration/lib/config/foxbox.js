'use strict';

module.exports = {

  nupnp_server : {
    param : 'philips_hue;nupnp_url',
    id :'111788fffe230b96',
    url : 'http://localhost',
    port: '8002'
  },

  credential :  {
    'email': 'a@b.com',
    'name': 'admin',
    'password': '87654321'
  },

  pagekite : {
    'r' : 'https://knilxof.org:4443',
    't' : 'knilxof.org:443',
    's' : 'foxbox',
  },

  foxbox : { 
    url : 'http://localhost:3000'
  },

  ipCamera : {
    ip: 'localhost',
    port: '8111',
    udn: 'ae67e622-7a66-465e-bab0-aaaaaaaaaaaa',
    description: 'descriptionurl',
    usn: 'urn:cellvision:service:Null:1'
  },

  webpush : {
    ip: 'http://localhost',
    port: '8112',
    endpoint: '/endpoint'
  },

  philips_hue : {
    url: 'localhost',
    port: 8001
  }
};
