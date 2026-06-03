{-# LANGUAGE FlexibleContexts    #-}
{-# LANGUAGE ScopedTypeVariables #-}

module Auth.Webauthn
  ( beginRegistration
  , completeRegistration
  , beginAuthentication
  , completeAuthentication
  ) where

import           Control.Monad            (when)
import           Control.Monad.Except     (throwError)
import           Control.Monad.IO.Class   (liftIO)
import           Control.Monad.Reader     (asks)
import           Crypto.Hash              (Digest, SHA256, hash)
import qualified Crypto.WebAuthn          as WA
import           Data.Aeson               (Result (..), Value, fromJSON, object,
                                           (.=))
import qualified Data.ByteString.Lazy.Char8 as LBC
import qualified Data.List.NonEmpty       as NE
import           Data.Text                (Text)
import           Data.Text.Encoding       (encodeUtf8)
import           Data.Time                (addUTCTime, getCurrentTime)
import qualified Data.Validation          as V
import           Database.Persist
import           Servant                  (err400, err401, err404, errBody)
import           Time.System              (dateCurrent)

import           Auth.Config              (Config (..))
import           Auth.Crypto.Token        (randomToken)
import           Auth.Models
import           Auth.Types               (AppEnv (..), AppM, runDB)


rpEntity :: Config -> WA.CredentialRpEntity
rpEntity cfg = WA.CredentialRpEntity
  { WA.creId   = Just (WA.RpId (cfgRpId cfg))
  , WA.creName = WA.RelyingPartyName (cfgRpName cfg)
  }

origin :: Config -> WA.Origin
origin cfg = WA.Origin (cfgOrigin cfg)

rpIdHash :: Config -> WA.RpIdHash
rpIdHash cfg = WA.RpIdHash (hash (encodeUtf8 (cfgRpId cfg)) :: Digest SHA256)

mkUserEntity :: User -> WA.CredentialUserEntity
mkUserEntity user = WA.CredentialUserEntity
  { WA.cueId          = WA.UserHandle (userUserHandle user)
  , WA.cueName        = WA.UserAccountName (userEmail user)
  , WA.cueDisplayName = WA.UserAccountDisplayName (userEmail user)
  }

registrationOptions
  :: Config -> WA.CredentialUserEntity -> WA.Challenge
  -> WA.CredentialOptions 'WA.Registration
registrationOptions cfg user challenge = WA.CredentialOptionsRegistration
  { WA.corRp                  = rpEntity cfg
  , WA.corUser                = user
  , WA.corChallenge           = challenge
  , WA.corPubKeyCredParams    =
      [ WA.CredentialParameters WA.CredentialTypePublicKey WA.CoseAlgorithmES256
      , WA.CredentialParameters WA.CredentialTypePublicKey WA.CoseAlgorithmRS256
      ]
  , WA.corTimeout             = Nothing
  , WA.corExcludeCredentials  = []
  , WA.corAuthenticatorSelection = Just WA.AuthenticatorSelectionCriteria
      { WA.ascAuthenticatorAttachment = Nothing
      , WA.ascResidentKey             = WA.ResidentKeyRequirementPreferred
      , WA.ascUserVerification        = WA.UserVerificationRequirementPreferred
      }
  , WA.corAttestation         = WA.AttestationConveyancePreferenceNone
  , WA.corExtensions          = Nothing
  }

authenticationOptions
  :: Config -> [Credential] -> WA.Challenge
  -> WA.CredentialOptions 'WA.Authentication
authenticationOptions cfg creds challenge = WA.CredentialOptionsAuthentication
  { WA.coaRpId             = Just (WA.RpId (cfgRpId cfg))
  , WA.coaTimeout          = Nothing
  , WA.coaChallenge        = challenge
  , WA.coaAllowCredentials = map descriptor creds
  , WA.coaUserVerification = WA.UserVerificationRequirementPreferred
  , WA.coaExtensions       = Nothing
  }
  where
    descriptor c = WA.CredentialDescriptor
      { WA.cdTyp        = WA.CredentialTypePublicKey
      , WA.cdId         = WA.CredentialId (credentialCredentialId c)
      , WA.cdTransports = Nothing
      }


storeChallenge :: Text -> Maybe UserId -> WA.Challenge -> AppM Text
storeChallenge purpose mUid challenge = do
  cfg    <- asks envConfig
  handle <- liftIO (randomToken 24)
  now    <- liftIO getCurrentTime
  let expires = addUTCTime (fromIntegral (cfgCeremonyTtlSeconds cfg)) now
  _ <- runDB $ insert (AuthCeremony handle purpose (WA.unChallenge challenge) mUid expires)
  pure handle

loadChallenge :: Text -> Text -> AppM (AuthCeremony)
loadChallenge purpose handle = do
  now  <- liftIO getCurrentTime
  mcer <- runDB $ getBy (UniqueCeremonyHandle handle)
  case mcer of
    Just (Entity cid c)
      | authCeremonyPurpose c == purpose && authCeremonyExpiresAt c >= now -> do
          runDB (delete cid)
          pure c
    _ -> throwError err400 { errBody = "unknown or expired ceremony" }


beginRegistration :: UserId -> User -> AppM Value
beginRegistration uid user = do
  cfg       <- asks envConfig
  challenge <- liftIO WA.generateChallenge
  let options = registrationOptions cfg (mkUserEntity user) challenge
  handle    <- storeChallenge "register" (Just uid) challenge
  pure $ object
    [ "handle"    .= handle
    , "publicKey" .= WA.wjEncodeCredentialOptionsRegistration options
    ]

completeRegistration :: UserId -> User -> Text -> Value -> AppM ()
completeRegistration uid user handle credentialJson = do
  cfg <- asks envConfig
  c   <- loadChallenge "register" handle
  cred <- decodeRegistration credentialJson
  let challenge = WA.Challenge (authCeremonyChallenge c)
      options   = registrationOptions cfg (mkUserEntity user) challenge
  now <- liftIO dateCurrent
  case WA.verifyRegistrationResponse (NE.singleton (origin cfg)) (rpIdHash cfg) mempty now options cred of
    V.Failure errs ->
      throwError err400 { errBody = "registration failed: " <> bsShow (NE.head errs) }
    V.Success result -> do
      let entry = WA.rrEntry result
          cid   = WA.unCredentialId (WA.ceCredentialId entry)
      existing <- runDB $ getBy (UniqueCredentialId cid)
      when (maybe False (\(Entity _ e) -> credentialUserId e /= uid) existing) $
        throwError err400 { errBody = "credential already registered to another account" }
      now' <- liftIO getCurrentTime
      runDB $ repsertCredential cid Credential
        { credentialUserId       = uid
        , credentialCredentialId = cid
        , credentialPublicKey    = WA.unPublicKeyBytes (WA.cePublicKeyBytes entry)
        , credentialSignCounter  = fromIntegral (WA.unSignatureCounter (WA.ceSignCounter entry))
        , credentialTransports   = "[]"
        , credentialUserHandle   = WA.unUserHandle (WA.ceUserHandle entry)
        , credentialCreatedAt    = now'
        }
  where
    repsertCredential cid c = do
      existing <- getBy (UniqueCredentialId cid)
      case existing of
        Just (Entity k _) -> replace k c
        Nothing           -> insert_ c

decodeRegistration :: Value -> AppM (WA.Credential 'WA.Registration 'True)
decodeRegistration v =
  case fromJSON v of
    Error e -> throwError err400 { errBody = "could not parse credential: " <> bsShow e }
    Success wj -> case WA.wjDecodeCredentialRegistration wj of
      Left err -> throwError err400 { errBody = "invalid credential: " <> bsShow err }
      Right c  -> pure c


beginAuthentication :: Text -> AppM Value
beginAuthentication email = do
  cfg   <- asks envConfig
  muser <- runDB $ getBy (UniqueUserEmail email)
  case muser of
    Nothing -> throwError err404 { errBody = "no such user" }
    Just (Entity uid _) -> do
      creds <- runDB $ map entityVal <$> selectList [CredentialUserId ==. uid] []
      when (null creds) $ throwError err404 { errBody = "user has no passkeys" }
      challenge <- liftIO WA.generateChallenge
      let options = authenticationOptions cfg creds challenge
      handle    <- storeChallenge "authenticate" (Just uid) challenge
      pure $ object
        [ "handle"    .= handle
        , "publicKey" .= WA.wjEncodeCredentialOptionsAuthentication options
        ]

completeAuthentication :: Text -> Value -> AppM UserId
completeAuthentication handle credentialJson = do
  cfg  <- asks envConfig
  c    <- loadChallenge "authenticate" handle
  cred <- decodeAuthentication credentialJson
  uid  <- maybe (throwError err400 { errBody = "ceremony has no user" }) pure (authCeremonyUserId c)
  creds <- runDB $ map entityVal <$> selectList [CredentialUserId ==. uid] []
  let credId = WA.unCredentialId (WA.cIdentifier cred)
  stored <- runDB $ getBy (UniqueCredentialId credId)
  case stored of
    Nothing -> throwError err401 { errBody = "unknown credential" }
    Just (Entity ck storedCred) -> do
      let challenge = WA.Challenge (authCeremonyChallenge c)
          options   = authenticationOptions cfg creds challenge
          entry     = toEntry storedCred
      case WA.verifyAuthenticationResponse
             (NE.singleton (origin cfg)) (rpIdHash cfg)
             (Just (WA.ceUserHandle entry)) entry options cred of
        V.Failure errs ->
          throwError err401 { errBody = "authentication failed: " <> bsShow (NE.head errs) }
        V.Success (WA.AuthenticationResult sigResult) -> do
          case sigResult of
            WA.SignatureCounterZero            -> pure ()
            WA.SignatureCounterUpdated counter ->
              runDB $ update ck [CredentialSignCounter =. fromIntegral (WA.unSignatureCounter counter)]
            WA.SignatureCounterPotentiallyCloned ->
              throwError err401 { errBody = "credential may be cloned" }
          pure uid
  where
    toEntry sc = WA.CredentialEntry
      { WA.ceCredentialId  = WA.CredentialId (credentialCredentialId sc)
      , WA.ceUserHandle    = WA.UserHandle (credentialUserHandle sc)
      , WA.cePublicKeyBytes = WA.PublicKeyBytes (credentialPublicKey sc)
      , WA.ceSignCounter   = WA.SignatureCounter (fromIntegral (credentialSignCounter sc))
      , WA.ceTransports    = []
      }

decodeAuthentication :: Value -> AppM (WA.Credential 'WA.Authentication 'True)
decodeAuthentication v =
  case fromJSON v of
    Error e -> throwError err400 { errBody = "could not parse credential: " <> bsShow e }
    Success wj -> case WA.wjDecodeCredentialAuthentication wj of
      Left err -> throwError err400 { errBody = "invalid credential: " <> bsShow err }
      Right c  -> pure c

bsShow :: Show a => a -> LBC.ByteString
bsShow = LBC.pack . show
