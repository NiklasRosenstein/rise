/// <reference path="../pb_data/types.d.ts" />
migrate((app) => {
  const collection = new Collection({
    "createRule": " @request.auth.id != \"\"",
    "deleteRule": "@request.auth.id != \"\" && (owners.id ?= @request.auth.id)",
    "fields": [
      {
        "autogeneratePattern": "[a-z0-9]{15}",
        "hidden": false,
        "id": "text3208210256",
        "max": 15,
        "min": 15,
        "name": "id",
        "pattern": "^[a-z0-9]+$",
        "presentable": false,
        "primaryKey": true,
        "required": true,
        "system": true,
        "type": "text"
      },
      {
        "autogeneratePattern": "",
        "hidden": false,
        "id": "text1579384326",
        "max": 255,
        "min": 1,
        "name": "name",
        "pattern": "^[a-z0-9-]+$",
        "presentable": false,
        "primaryKey": false,
        "required": true,
        "system": false,
        "type": "text"
      },
      {
        "cascadeDelete": false,
        "collectionId": "_pb_users_auth_",
        "hidden": false,
        "id": "relation1168167679",
        "maxSelect": 0,
        "minSelect": 0,
        "name": "members",
        "presentable": false,
        "required": false,
        "system": false,
        "type": "relation"
      },
      {
        "cascadeDelete": false,
        "collectionId": "_pb_users_auth_",
        "hidden": false,
        "id": "relation1114804986",
        "maxSelect": 999,
        "minSelect": 0,
        "name": "owners",
        "presentable": false,
        "required": false,
        "system": false,
        "type": "relation"
      }
    ],
    "id": "pbc_1568971955",
    "indexes": [
      "CREATE UNIQUE INDEX `idx_2vui7Ax7yx` ON `teams` (`name`)"
    ],
    "listRule": "@request.auth.id != \"\" && (members.id ?= @request.auth.id || owners.id ?= @request.auth.id)",
    "name": "teams",
    "system": false,
    "type": "base",
    "updateRule": "@request.auth.id != \"\" && (owners.id ?= @request.auth.id)",
    "viewRule": "@request.auth.id != \"\" && (members.id ?= @request.auth.id || owners.id ?= @request.auth.id)"
  });

  return app.save(collection);
}, (app) => {
  const collection = app.findCollectionByNameOrId("pbc_1568971955");

  return app.delete(collection);
})
