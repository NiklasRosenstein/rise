/// <reference path="../pb_data/types.d.ts" />
migrate((app) => {
  const collection = app.findCollectionByNameOrId("pbc_484305853")

  // update field
  collection.fields.addAt(2, new Field({
    "hidden": false,
    "id": "select1542800728",
    "maxSelect": 1,
    "name": "status",
    "presentable": false,
    "required": true,
    "system": false,
    "type": "select",
    "values": [
      "Stopped",
      "Running",
      "Failed",
      "Deploying"
    ]
  }))

  return app.save(collection)
}, (app) => {
  const collection = app.findCollectionByNameOrId("pbc_484305853")

  // update field
  collection.fields.addAt(2, new Field({
    "hidden": false,
    "id": "select1542800728",
    "maxSelect": 1,
    "name": "field",
    "presentable": false,
    "required": true,
    "system": false,
    "type": "select",
    "values": [
      "Stopped",
      "Running",
      "Deployed",
      "Failed"
    ]
  }))

  return app.save(collection)
})
