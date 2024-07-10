db.Album.createIndex({ AlbumId: 1 })
db.Album.createIndex({ ArtistId: 1 })
db.Artist.createIndex({ ArtistId: 1 })
db.Customer.createIndex({ CustomerId: 1 })
db.Customer.createIndex({ SupportRepId: 1 })
db.Employee.createIndex({ EmployeeId: 1 })
db.Employee.createIndex({ ReportsTo: 1 })
db.Genre.createIndex({ GenreId: 1 })
db.Invoice.createIndex({ CustomerId: 1 })
db.Invoice.createIndex({ InvoiceId: 1 })
db.InvoiceLine.createIndex({ InvoiceId: 1 })
db.InvoiceLine.createIndex({ TrackId: 1 })
db.MediaType.createIndex({ MediaTypeId: 1 })
db.Playlist.createIndex({ PlaylistId: 1 })
db.PlaylistTrack.createIndex({ PlaylistId: 1 })
db.PlaylistTrack.createIndex({ TrackId: 1 })
db.Track.createIndex({ AlbumId: 1 })
db.Track.createIndex({ GenreId: 1 })
db.Track.createIndex({ MediaTypeId: 1 })
db.Track.createIndex({ TrackId: 1 })