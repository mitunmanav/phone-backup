plugins {
    id("com.android.application") version "9.3.1"
}

// Set only by the Release workflow. Every other build signs with the debug key.
val keystore: String? = System.getenv("COMPANION_KEYSTORE")

android {
    namespace = "org.phonebackup.companion"
    compileSdk = 36

    defaultConfig {
        applicationId = "org.phonebackup.companion"
        minSdk = 23
        targetSdk = 36
        versionCode = 1
        versionName = "0.1"
    }

    signingConfigs {
        if (keystore != null) {
            create("release") {
                storeFile = file(keystore)
                storePassword = System.getenv("COMPANION_KEY_PASSWORD")
                keyAlias = "companion"
                keyPassword = System.getenv("COMPANION_KEY_PASSWORD")
            }
        }
    }

    buildTypes {
        release {
            signingConfig = signingConfigs.getByName(if (keystore != null) "release" else "debug")
        }
    }
}
