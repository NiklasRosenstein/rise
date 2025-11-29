/// <reference path="../pb_data/types.d.ts" />
migrate((app) => {
  const collection = app.findCollectionByNameOrId("pbc_1568971955")

  // update collection data
  unmarshal({
    "createRule": "  @request.auth.id != \"\" && @request.body.owners:each ?= @request.auth.id"
  }, collection)

  return app.save(collection)
}, (app) => {
  const collection = app.findCollectionByNameOrId("pbc_1568971955")

  // update collection data
  unmarshal({
    "createRule": " @request.auth.id != \"\""
  }, collection)

  return app.save(collection)
})
