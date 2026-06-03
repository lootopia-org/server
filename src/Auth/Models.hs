{-# LANGUAGE DerivingStrategies         #-}
{-# LANGUAGE GADTs                      #-}
{-# LANGUAGE GeneralizedNewtypeDeriving #-}
{-# LANGUAGE QuasiQuotes                #-}
{-# LANGUAGE StandaloneDeriving         #-}
{-# LANGUAGE TemplateHaskell            #-}
{-# LANGUAGE TypeFamilies               #-}
{-# LANGUAGE UndecidableInstances       #-}

module Auth.Models where

import           Data.ByteString     (ByteString)
import           Data.Text           (Text)
import           Data.Time           (UTCTime)
import           Database.Persist.TH

share [mkPersist sqlSettings, mkMigrate "migrateAll"] [persistLowerCase|
User
    email          Text
    emailVerified  Bool
    passwordSalt   ByteString
    passwordHash   ByteString
    userHandle     ByteString      
    totpSecret     ByteString Maybe 
    totpEnabled    Bool
    createdAt      UTCTime
    UniqueUserEmail email
    UniqueUserHandle userHandle
    deriving Show

EmailToken
    userId     UserId
    token      Text
    expiresAt  UTCTime
    UniqueEmailToken token
    deriving Show

Session
    userId      UserId
    token       Text
    mfaPending  Bool
    expiresAt   UTCTime
    createdAt   UTCTime
    UniqueSessionToken token
    deriving Show

Credential
    userId        UserId
    credentialId  ByteString
    publicKey     ByteString
    signCounter   Int
    transports    Text          
    userHandle    ByteString
    createdAt     UTCTime
    UniqueCredentialId credentialId
    deriving Show

AuthCeremony
    handle     Text
    purpose    Text             
    challenge  ByteString
    userId     UserId Maybe
    expiresAt  UTCTime
    UniqueCeremonyHandle handle
    deriving Show
|]
